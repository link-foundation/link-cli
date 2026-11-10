using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>Unit tests of the draft <see cref="RaftNode"/>. The first five mirror
/// the tests in rust/src/engine/raft.rs; the rest drive single messages into a
/// node to cover each branch of the protocol.</summary>
public class RaftNodeTests
{
    private static readonly JournalVersion Empty = new(0, Array.Empty<string>());

    private static void Deliver(RaftNode[] nodes, ulong now)
    {
        while (true)
        {
            var messages = nodes.SelectMany(node => node.TakeMessages()).ToList();
            if (messages.Count == 0) return;
            foreach (var message in messages) nodes[message.To].Step(now, message);
        }
    }

    private static RaftNode[] Cluster(bool preVote, bool checkQuorum, int maxEntriesPerMessage = 32) =>
        Enumerable.Range(0, 3)
            .Select(id => new RaftNode(new RaftConfig(id, 3) { PreVote = preVote, CheckQuorum = checkQuorum, MaxEntriesPerMessage = maxEntriesPerMessage }, 0))
            .ToArray();

    private static RaftCommand.CompareExchange Exchange(ulong id) => new(id, ["a"], [Empty], [new[] { "x" }]);

    private static RaftEnvelope Envelope(int from, int to, RaftMessage message) => new(from, to, message);

    // -----------------------------------------------------------------------
    // Ports of the Rust unit tests
    // -----------------------------------------------------------------------

    [Fact]
    public void ASingleMemberElectsItselfAndCommits()
    {
        var node = new RaftNode(new RaftConfig(0, 1), 0);
        node.Tick(1000);
        Assert.Equal(RaftRole.Leader, node.Role);
        Assert.True(node.Propose(1000, new RaftCommand.Read(7, ["a"])));
        Assert.Equal(new RaftOutcome.Read([Empty]), node.TakeOutcome(7));
    }

    [Fact]
    public void ALeaderReplicatesCompareExchangeToEveryMember()
    {
        var nodes = Cluster(true, true);
        nodes[0].Tick(1000);
        Deliver(nodes, 1000);
        Assert.Equal(RaftRole.Leader, nodes[0].Role);
        Assert.True(nodes[1].Propose(1001, Exchange(1)));
        Deliver(nodes, 1001);
        Assert.Equal(2UL, nodes[0].CommitIndex);
        // Followers learn the commit index from the next heartbeat.
        Assert.Null(nodes[1].TakeOutcome(1));
        nodes[0].Tick(1011);
        Deliver(nodes, 1011);
        Assert.Equal(new RaftOutcome.Exchanged(true), nodes[1].TakeOutcome(1));
        foreach (var node in nodes) Assert.Equal(new[] { "x" }, node.State.Journals["a"].Entries);
        // The same compare-exchange again sees a newer generation.
        Assert.True(nodes[2].Propose(1012, Exchange(2)));
        Deliver(nodes, 1012);
        nodes[0].Tick(1022);
        Deliver(nodes, 1022);
        Assert.Equal(new RaftOutcome.Exchanged(false), nodes[2].TakeOutcome(2));
        Assert.Equal(new (ulong, ulong)[] { (1, 1) }, nodes[0].State.Exchanges);
    }

    [Fact]
    public void AMemberInsideALeaderLeaseIgnoresADisruptiveCampaign()
    {
        var nodes = Cluster(true, true);
        nodes[0].Tick(1000);
        Deliver(nodes, 1000);
        // Node 2 cannot hear anything, times out and campaigns.
        nodes[2].Tick(5000);
        var campaign = nodes[2].TakeMessages();
        Assert.NotEmpty(campaign);
        Assert.All(campaign, message => Assert.True(message.Message is RaftMessage.Vote { Pre: true }));
        foreach (var message in campaign) nodes[message.To].Step(1005, message);
        // Neither member replies or raises its term.
        Assert.Empty(nodes[0].TakeMessages());
        Assert.Empty(nodes[1].TakeMessages());
        Assert.Equal((1UL, RaftRole.Leader), (nodes[0].Term, nodes[0].Role));
        Assert.Equal(1UL, nodes[2].Term);
    }

    [Fact]
    public void ALeaderThatHearsNoMajorityStepsDownUnderCheckQuorum()
    {
        var nodes = Cluster(true, true);
        nodes[0].Tick(1000);
        Deliver(nodes, 1000);
        nodes[0].Tick(1100);
        Assert.Equal(RaftRole.Leader, nodes[0].Role);
        nodes[0].TakeMessages();
        nodes[0].Tick(1200);
        Assert.Equal(RaftRole.Follower, nodes[0].Role);
    }

    [Fact]
    public void ARestartedMemberRebuildsItsStateFromTheLog()
    {
        var nodes = Cluster(false, false);
        nodes[0].Tick(1000);
        Deliver(nodes, 1000);
        Assert.Equal(RaftRole.Leader, nodes[0].Role);
        nodes[0].Propose(1000, Exchange(1));
        Deliver(nodes, 1000);
        nodes[1].Restart(1001);
        Assert.Equal(new JournalState(), nodes[1].State);
        nodes[0].Tick(1011);
        Deliver(nodes, 1011);
        Assert.Equal(nodes[0].State, nodes[1].State);
        Assert.Equal(1UL, nodes[1].HardState.Term);
    }

    // -----------------------------------------------------------------------
    // Configuration and message validation
    // -----------------------------------------------------------------------

    [Fact]
    public void TheConstructorRejectsInvalidConfigurations()
    {
        Assert.Throws<ArgumentOutOfRangeException>(() => new RaftNode(new RaftConfig(0, 0), 0));
        Assert.Throws<ArgumentOutOfRangeException>(() => new RaftNode(new RaftConfig(-1, 3), 0));
        Assert.Throws<ArgumentOutOfRangeException>(() => new RaftNode(new RaftConfig(3, 3), 0));
        Assert.Throws<ArgumentOutOfRangeException>(() => new RaftNode(new RaftConfig(0, 3) { MaxEntriesPerMessage = -1 }, 0));
        Assert.Throws<ArgumentException>(() => new RaftNode(new RaftConfig(0, 3) { ElectionTimeout = 10, HeartbeatInterval = 10 }, 0));
        var config = new RaftConfig(2, 3);
        Assert.Equal((2, 3, 3UL, 100UL, 10UL, true, true, 32), (config.Id, config.Members, config.Seed, config.ElectionTimeout,
            config.HeartbeatInterval, config.PreVote, config.CheckQuorum, config.MaxEntriesPerMessage));
    }

    [Fact]
    public void StepIgnoresMessagesFromItselfAndFromUnknownMembers()
    {
        var node = new RaftNode(new RaftConfig(1, 3), 0);
        foreach (var from in new[] { -1, 1, 3 })
            node.Step(0, Envelope(from, 1, new RaftMessage.Append(5, 0, 0, [], 0)));
        Assert.Equal(new RaftHardState(), node.HardState);
        Assert.Null(node.Leader);
        Assert.Empty(node.TakeMessages());
        Assert.Equal(1, node.Id);
    }

    [Fact]
    public void TimersDoNothingBeforeTheyAreDue()
    {
        var follower = new RaftNode(new RaftConfig(1, 3), 0);
        follower.Tick(50);
        Assert.Equal(RaftRole.Follower, follower.Role);
        Assert.Empty(follower.TakeMessages());
        var nodes = Cluster(true, true);
        nodes[0].Tick(1000);
        Deliver(nodes, 1000);
        nodes[0].Tick(1005);
        Assert.Empty(nodes[0].TakeMessages());
        nodes[0].Tick(1010);
        Assert.Equal(2, nodes[0].TakeMessages().Count);
    }

    [Fact]
    public void WithoutCheckQuorumALeaderNeverStepsDown()
    {
        var nodes = Cluster(true, false);
        nodes[0].Tick(1000);
        Deliver(nodes, 1000);
        foreach (var now in new ulong[] { 1100, 1200, 100_000 })
        {
            nodes[0].Tick(now);
            Assert.Equal(RaftRole.Leader, nodes[0].Role);
            Assert.Equal(2, nodes[0].TakeMessages().Count);
        }
    }

    [Fact]
    public void ASingleMemberWithoutPreVoteBecomesCandidateThenLeader()
    {
        var node = new RaftNode(new RaftConfig(0, 1) { PreVote = false }, 0);
        node.Tick(1000);
        Assert.Equal((RaftRole.Leader, 1UL, (int?)0), (node.Role, node.Term, node.Leader));
        Assert.Equal(new RaftHardState(1, 0), node.HardState);
        Assert.Equal(1UL, node.CommitIndex);
        Assert.Equal(1UL, node.AppliedIndex);
    }

    // -----------------------------------------------------------------------
    // Terms and votes
    // -----------------------------------------------------------------------

    [Fact]
    public void MessagesOfAStaleTermAreAnsweredWithTheNewerTermOrIgnored()
    {
        var node = new RaftNode(new RaftConfig(1, 3), 0);
        node.Step(10, Envelope(0, 1, new RaftMessage.Append(2, 0, 0, [], 0)));
        Assert.Equal((RaftRole.Follower, 2UL, (int?)0), (node.Role, node.Term, node.Leader));
        Assert.Equal([Envelope(1, 0, new RaftMessage.AppendReply(2, true, 0))], node.TakeMessages());
        node.Step(10, Envelope(2, 1, new RaftMessage.Append(1, 0, 0, [], 0)));
        node.Step(10, Envelope(2, 1, new RaftMessage.Vote(true, 1, 0, 0)));
        node.Step(10, Envelope(2, 1, new RaftMessage.VoteReply(false, 1, true)));
        node.Step(10, Envelope(2, 1, new RaftMessage.AppendReply(1, true, 0)));
        Assert.Equal(
            [Envelope(1, 2, new RaftMessage.AppendReply(2, false, 0)), Envelope(1, 2, new RaftMessage.VoteReply(true, 2, false))],
            node.TakeMessages());
        Assert.Equal(2UL, node.Term);
    }

    [Fact]
    public void AFollowerRefusesVotesInsideTheLeaseAndGrantsThemAfterIt()
    {
        var node = new RaftNode(new RaftConfig(1, 3), 0);
        node.Step(10, Envelope(0, 1, new RaftMessage.Append(2, 0, 0, [], 0)));
        node.TakeMessages();
        node.Step(50, Envelope(2, 1, new RaftMessage.Vote(false, 3, 0, 0)));
        Assert.Empty(node.TakeMessages());
        Assert.Equal(2UL, node.Term);
        node.Step(500, Envelope(2, 1, new RaftMessage.Vote(false, 3, 0, 0)));
        Assert.Equal([Envelope(1, 2, new RaftMessage.VoteReply(false, 3, true))], node.TakeMessages());
        Assert.Equal(new RaftHardState(3, 2), node.HardState);
        Assert.Null(node.Leader);
    }

    [Fact]
    public void WithoutCheckQuorumAFollowerHasNoLease()
    {
        var nodes = Cluster(false, false);
        nodes[0].Tick(1000);
        Deliver(nodes, 1000);
        nodes[1].Step(1005, Envelope(2, 1, new RaftMessage.Vote(false, 5, 10, 10)));
        Assert.Equal([Envelope(1, 2, new RaftMessage.VoteReply(false, 5, true))], nodes[1].TakeMessages());
        Assert.Equal(new RaftHardState(5, 2), nodes[1].HardState);
    }

    [Fact]
    public void APreVoteNeverChangesTheVoterAndNeedsAnUpToDateLog()
    {
        var node = new RaftNode(new RaftConfig(1, 3), 0);
        node.Step(0, Envelope(0, 1, new RaftMessage.Vote(true, 1, 0, 0)));
        Assert.Equal([Envelope(1, 0, new RaftMessage.VoteReply(true, 1, true))], node.TakeMessages());
        Assert.Equal(new RaftHardState(), node.HardState);
        // Give the node an entry of term 1, then let the lease expire.
        node.Step(0, Envelope(0, 1, new RaftMessage.Append(1, 0, 0, [new RaftEntry(1, new RaftCommand.Noop())], 0)));
        node.TakeMessages();
        node.Step(1000, Envelope(2, 1, new RaftMessage.Vote(true, 2, 0, 0)));
        node.Step(1000, Envelope(2, 1, new RaftMessage.Vote(true, 1, 1, 1)));
        node.Step(1000, Envelope(2, 1, new RaftMessage.Vote(true, 2, 0, 5)));
        node.Step(1000, Envelope(2, 1, new RaftMessage.Vote(true, 2, 1, 1)));
        Assert.Equal(
            [
                Envelope(1, 2, new RaftMessage.VoteReply(true, 1, false)),
                Envelope(1, 2, new RaftMessage.VoteReply(true, 1, false)),
                Envelope(1, 2, new RaftMessage.VoteReply(true, 2, true)),
                Envelope(1, 2, new RaftMessage.VoteReply(true, 2, true)),
            ],
            node.TakeMessages());
        Assert.Equal(new RaftHardState(1, null), node.HardState);
    }

    [Fact]
    public void AVoteGoesToOneCandidatePerTermAndOnlyToAnUpToDateLog()
    {
        var node = new RaftNode(new RaftConfig(1, 3) { PreVote = false, CheckQuorum = false }, 0);
        node.Step(0, Envelope(0, 1, new RaftMessage.Vote(false, 1, 0, 0)));
        node.Step(0, Envelope(2, 1, new RaftMessage.Vote(false, 1, 0, 0)));
        node.Step(0, Envelope(0, 1, new RaftMessage.Vote(false, 1, 0, 0)));
        Assert.Equal(
            [
                Envelope(1, 0, new RaftMessage.VoteReply(false, 1, true)),
                Envelope(1, 2, new RaftMessage.VoteReply(false, 1, false)),
                Envelope(1, 0, new RaftMessage.VoteReply(false, 1, true)),
            ],
            node.TakeMessages());
        Assert.Equal(new RaftHardState(1, 0), node.HardState);
        node.Step(0, Envelope(0, 1, new RaftMessage.Append(1, 0, 0, [new RaftEntry(1, new RaftCommand.Noop())], 0)));
        node.TakeMessages();
        node.Step(0, Envelope(2, 1, new RaftMessage.Vote(false, 2, 0, 0)));
        Assert.Equal([Envelope(1, 2, new RaftMessage.VoteReply(false, 2, false))], node.TakeMessages());
        Assert.Equal(new RaftHardState(2, null), node.HardState);
    }

    [Fact]
    public void APreCandidateCountsOnlyGrantedPreVotesOfItsNextTerm()
    {
        var follower = new RaftNode(new RaftConfig(1, 3), 0);
        follower.Step(0, Envelope(0, 1, new RaftMessage.VoteReply(false, 0, true)));
        Assert.Equal(RaftRole.Follower, follower.Role);

        var node = new RaftNode(new RaftConfig(0, 3), 0);
        node.Tick(1000);
        Assert.Equal((RaftRole.PreCandidate, 0UL), (node.Role, node.Term));
        Assert.Equal(
            [Envelope(0, 1, new RaftMessage.Vote(true, 1, 0, 0)), Envelope(0, 2, new RaftMessage.Vote(true, 1, 0, 0))],
            node.TakeMessages());
        node.Step(1000, Envelope(1, 0, new RaftMessage.VoteReply(false, 0, true)));
        node.Step(1000, Envelope(1, 0, new RaftMessage.VoteReply(true, 2, true)));
        Assert.Equal((RaftRole.PreCandidate, 0UL), (node.Role, node.Term));
        node.Step(1000, Envelope(1, 0, new RaftMessage.VoteReply(true, 1, true)));
        Assert.Equal((RaftRole.Candidate, new RaftHardState(1, 0)), (node.Role, node.HardState));
        Assert.Equal(
            [Envelope(0, 1, new RaftMessage.Vote(false, 1, 0, 0)), Envelope(0, 2, new RaftMessage.Vote(false, 1, 0, 0))],
            node.TakeMessages());

        // A pre-vote refused by a newer term turns a pre-candidate into a follower.
        var stale = new RaftNode(new RaftConfig(0, 3), 0);
        stale.Tick(1000);
        stale.Step(1000, Envelope(1, 0, new RaftMessage.VoteReply(true, 3, false)));
        Assert.Equal((RaftRole.Follower, 3UL, (int?)null), (stale.Role, stale.Term, stale.Leader));
    }

    [Fact]
    public void AFiveMemberCandidateNeedsThreeVotes()
    {
        var node = new RaftNode(new RaftConfig(0, 5) { PreVote = false }, 0);
        node.Tick(1000);
        Assert.Equal(4, node.TakeMessages().Count);
        node.Step(1000, Envelope(1, 0, new RaftMessage.VoteReply(false, 1, true)));
        node.Step(1000, Envelope(1, 0, new RaftMessage.VoteReply(false, 1, true)));
        Assert.Equal(RaftRole.Candidate, node.Role);
        node.Step(1000, Envelope(4, 0, new RaftMessage.VoteReply(false, 1, true)));
        Assert.Equal(RaftRole.Leader, node.Role);
        // A leader may propose a no-op of its own; nobody waits for it.
        Assert.True(node.Propose(1000, new RaftCommand.Noop()));
        Assert.Equal(2UL, node.LastLogIndex());
    }

    [Fact]
    public void ACandidateWinsWithAMajorityOfGrantedVotesOfItsTerm()
    {
        var node = new RaftNode(new RaftConfig(0, 3) { PreVote = false, CheckQuorum = false }, 0);
        node.Tick(1000);
        Assert.Equal((RaftRole.Candidate, new RaftHardState(1, 0)), (node.Role, node.HardState));
        node.TakeMessages();
        node.Step(1000, Envelope(1, 0, new RaftMessage.VoteReply(false, 1, false)));
        node.Step(1000, Envelope(1, 0, new RaftMessage.VoteReply(true, 1, true)));
        Assert.Equal(RaftRole.Candidate, node.Role);
        node.Step(1000, Envelope(2, 0, new RaftMessage.VoteReply(false, 1, true)));
        Assert.Equal((RaftRole.Leader, (int?)0), (node.Role, node.Leader));
        var noop = new RaftEntry[] { new(1, new RaftCommand.Noop()) };
        Assert.Equal(
            [Envelope(0, 1, new RaftMessage.Append(1, 0, 0, noop, 0)), Envelope(0, 2, new RaftMessage.Append(1, 0, 0, noop, 0))],
            node.TakeMessages());
    }

    [Fact]
    public void ACandidateStepsDownForALeaderOfItsTerm()
    {
        var node = new RaftNode(new RaftConfig(0, 3) { PreVote = false }, 0);
        node.Tick(1000);
        node.TakeMessages();
        node.Step(1000, Envelope(1, 0, new RaftMessage.Append(1, 0, 0, [], 0)));
        Assert.Equal((RaftRole.Follower, 1UL, (int?)1), (node.Role, node.Term, node.Leader));
        Assert.Equal([Envelope(0, 1, new RaftMessage.AppendReply(1, true, 0))], node.TakeMessages());
    }

    // -----------------------------------------------------------------------
    // Proposals
    // -----------------------------------------------------------------------

    [Fact]
    public void ProposalsGoToTheKnownLeaderAndAreDroppedElsewhere()
    {
        var lonely = new RaftNode(new RaftConfig(2, 3), 0);
        Assert.False(lonely.Propose(0, new RaftCommand.Read(5, [])));
        Assert.Empty(lonely.TakeMessages());

        var nodes = Cluster(true, true);
        nodes[0].Tick(1000);
        Deliver(nodes, 1000);
        Assert.Equal((int?)0, nodes[1].Leader);
        var read = new RaftCommand.Read(1, ["a"]);
        Assert.True(nodes[1].Propose(1001, read));
        var forwarded = nodes[1].TakeMessages();
        Assert.Equal([Envelope(1, 0, new RaftMessage.Propose(new RaftCommand.Read(1, new List<string> { "a" })))], forwarded);
        // A follower drops a forwarded proposal: no proposal is retried.
        nodes[2].Step(1001, forwarded[0] with { To = 2 });
        Assert.Empty(nodes[2].TakeMessages());
        Assert.Equal(1UL, nodes[2].LastLogIndex());
        nodes[0].Step(1001, forwarded[0]);
        Assert.Equal(new RaftEntry(1, read), nodes[0].Log[^1]);
        // The caller gives up; the read still commits but nobody waits for it.
        nodes[1].Forget(1);
        Deliver(nodes, 1001);
        nodes[0].Tick(1011);
        Deliver(nodes, 1011);
        Assert.Equal(2UL, nodes[1].AppliedIndex);
        Assert.Null(nodes[1].TakeOutcome(1));
    }

    [Fact]
    public void ARestartDropsPendingOutcomesAndKeepsTheLog()
    {
        var node = new RaftNode(new RaftConfig(0, 1), 0);
        node.Tick(1000);
        node.Propose(1000, new RaftCommand.Read(1, []));
        node.Propose(1000, Exchange(2));
        var log = node.Log.ToArray();
        node.Restart(1001);
        Assert.Equal((RaftRole.Follower, 0UL, 0UL, (int?)null), (node.Role, node.CommitIndex, node.AppliedIndex, node.Leader));
        Assert.Null(node.TakeOutcome(1));
        Assert.Null(node.TakeOutcome(2));
        Assert.Equal(log, node.Log);
        Assert.Equal(new RaftHardState(1, 0), node.HardState);
        node.Tick(5000);
        Assert.Equal((RaftRole.Leader, 2UL), (node.Role, node.Term));
        Assert.Equal(1UL, node.State.Revision);
    }

    // -----------------------------------------------------------------------
    // Log replication
    // -----------------------------------------------------------------------

    [Fact]
    public void AFollowerRejectsGapsTruncatesConflictsAndNeverOverwritesCommittedEntries()
    {
        var node = new RaftNode(new RaftConfig(1, 3), 0);
        RaftEntry Entry(ulong term, ulong id) => new(term, new RaftCommand.Read(id, []));
        node.Step(0, Envelope(0, 1, new RaftMessage.Append(1, 5, 1, [], 0)));
        node.Step(0, Envelope(0, 1, new RaftMessage.Append(1, 0, 0, [Entry(1, 1), Entry(1, 2)], 0)));
        node.Step(0, Envelope(0, 1, new RaftMessage.Append(1, 2, 0, [], 0)));
        // A repeated entry is kept; a commit index only grows.
        node.Step(0, Envelope(0, 1, new RaftMessage.Append(1, 0, 0, [Entry(1, 1)], 1)));
        Assert.Equal(
            [
                Envelope(1, 0, new RaftMessage.AppendReply(1, false, 0)),
                Envelope(1, 0, new RaftMessage.AppendReply(1, true, 2)),
                Envelope(1, 0, new RaftMessage.AppendReply(1, false, 1)),
                Envelope(1, 0, new RaftMessage.AppendReply(1, true, 1)),
            ],
            node.TakeMessages());
        Assert.Equal((3, 1UL, 1UL), (node.Log.Count, node.CommitIndex, node.AppliedIndex));
        // A new leader replaces the uncommitted suffix.
        node.Step(0, Envelope(2, 1, new RaftMessage.Append(2, 1, 1, [Entry(2, 3)], 0)));
        Assert.Equal((3, 2UL, (int?)2), (node.Log.Count, node.Log[2].Term, node.Leader));
        // A commit index beyond the matched entries is capped.
        node.Step(0, Envelope(2, 1, new RaftMessage.Append(2, 2, 2, [], 10)));
        Assert.Equal((2UL, 2UL), (node.CommitIndex, node.AppliedIndex));
        Assert.Equal(
            [Envelope(1, 2, new RaftMessage.AppendReply(2, true, 2)), Envelope(1, 2, new RaftMessage.AppendReply(2, true, 2))],
            node.TakeMessages());
        var error = Assert.Throws<InvalidOperationException>(() =>
            node.Step(0, Envelope(0, 1, new RaftMessage.Append(3, 0, 0, [Entry(3, 4)], 0))));
        Assert.Contains("committed", error.Message, StringComparison.Ordinal);
    }

    [Fact]
    public void ALeaderBacksOffOnRejectionsAndSendsBoundedBatches()
    {
        var nodes = Cluster(false, false, maxEntriesPerMessage: 1);
        nodes[0].Tick(1000);
        Deliver(nodes, 1000);
        for (ulong id = 1; id <= 3; id++) nodes[0].Propose(1000, new RaftCommand.Read(id, []));
        Deliver(nodes, 1000);
        Assert.Equal(4UL, nodes[0].CommitIndex);
        Assert.All(nodes, node => Assert.Equal(nodes[0].Log, node.Log));
        // A rejection with a hint moves next back and resends one entry.
        nodes[0].Step(1000, Envelope(1, 0, new RaftMessage.AppendReply(1, false, 0)));
        Assert.Equal([Envelope(0, 1, new RaftMessage.Append(1, 0, 0, [nodes[0].Log[1]], 4))], nodes[0].TakeMessages());
        // A late success does not lower the match index, and next catches up.
        nodes[0].Step(1000, Envelope(1, 0, new RaftMessage.AppendReply(1, true, 1)));
        Assert.Equal([Envelope(0, 1, new RaftMessage.Append(1, 1, 1, [nodes[0].Log[2]], 4))], nodes[0].TakeMessages());
        Assert.Equal(4UL, nodes[0].CommitIndex);
        // Followers ignore append replies.
        nodes[1].Step(1000, Envelope(0, 1, new RaftMessage.AppendReply(1, true, 1)));
        Assert.Empty(nodes[1].TakeMessages());
    }

    [Fact]
    public void ALeaderCommitsEarlierTermsOnlyThroughAnEntryOfItsOwnTerm()
    {
        var node = new RaftNode(new RaftConfig(0, 3) { PreVote = false, CheckQuorum = false }, 0);
        node.Step(0, Envelope(1, 0, new RaftMessage.Append(1, 0, 0, [new RaftEntry(1, Exchange(9))], 0)));
        node.Tick(1000);
        node.Step(1000, Envelope(1, 0, new RaftMessage.VoteReply(false, 2, true)));
        Assert.Equal((RaftRole.Leader, 2UL, 2UL), (node.Role, node.Term, node.LastLogIndex()));
        node.TakeMessages();
        node.Step(1000, Envelope(2, 0, new RaftMessage.AppendReply(2, true, 1)));
        Assert.Equal(0UL, node.CommitIndex);
        node.TakeMessages();
        node.Step(1000, Envelope(2, 0, new RaftMessage.AppendReply(2, true, 2)));
        Assert.Equal((2UL, 2UL), (node.CommitIndex, node.AppliedIndex));
        Assert.Empty(node.TakeMessages());
        // Nobody waited here for the exchange proposed under the old leader.
        Assert.Null(node.TakeOutcome(9));
        Assert.Equal(new (ulong, ulong)[] { (1, 9) }, node.State.Exchanges);
    }

    // -----------------------------------------------------------------------
    // State machine and value equality
    // -----------------------------------------------------------------------

    [Fact]
    public void TheJournalStateAppliesReadsAndCompareExchanges()
    {
        var state = new JournalState();
        Assert.Equal(new RaftOutcome.Read([Empty]), state.Apply(new RaftCommand.Read(1, ["a"])));
        Assert.Equal(new RaftOutcome.Exchanged(false), state.Apply(new RaftCommand.CompareExchange(2, ["a", "b"], [Empty], [new[] { "x" }, null])));
        Assert.Equal(new RaftOutcome.Exchanged(false), state.Apply(new RaftCommand.CompareExchange(3, ["a"], [Empty], [new[] { "x" }, null])));
        Assert.Equal(new RaftOutcome.Exchanged(true), state.Apply(new RaftCommand.CompareExchange(4, ["a", "b"], [Empty, Empty], [new[] { "x" }, null])));
        Assert.Equal(new RaftOutcome.Exchanged(false), state.Apply(new RaftCommand.CompareExchange(5, ["a"], [Empty], [null])));
        Assert.Null(state.Apply(new RaftCommand.Noop()));
        Assert.Equal(1UL, state.Revision);
        Assert.Equal(["a"], state.Journals.Keys);
        Assert.Equal(new RaftOutcome.Read([new JournalVersion(1, ["x"]), Empty]), state.Apply(new RaftCommand.Read(6, ["a", "b"])));
        Assert.Equal(new (ulong, ulong)[] { (1, 4) }, state.Exchanges);
    }

    [Fact]
    public void JournalStatesCompareByValue()
    {
        static JournalState With(params RaftCommand[] commands)
        {
            var state = new JournalState();
            foreach (var command in commands) state.Apply(command);
            return state;
        }
        var state = With(new RaftCommand.CompareExchange(4, ["a"], [Empty], [new[] { "x" }]));
        var same = With(new RaftCommand.CompareExchange(4, ["a"], [Empty], [new List<string> { "x" }]));
        Assert.True(state.Equals(same));
        Assert.True(state.Equals((object)same));
        Assert.Equal(state.GetHashCode(), same.GetHashCode());
        Assert.False(state.Equals(null));
        Assert.False(state.Equals((object)"state"));
        Assert.NotEqual(state, new JournalState());
        Assert.NotEqual(state, With(new RaftCommand.CompareExchange(4, ["a"], [Empty], [new[] { "y" }])));
        Assert.NotEqual(state, With(new RaftCommand.CompareExchange(4, ["b"], [Empty], [new[] { "x" }])));
        Assert.NotEqual(state, With(new RaftCommand.CompareExchange(6, ["a"], [Empty], [new[] { "x" }])));
        Assert.NotEqual(state, With(new RaftCommand.CompareExchange(4, ["a", "b"], [Empty, Empty], [new[] { "x" }, new[] { "z" }])));
    }

    [Fact]
    public void CommandsOutcomesAndMessagesCompareByValue()
    {
        var read = new RaftCommand.Read(1, ["a"]);
        Assert.Equal(read, new RaftCommand.Read(1, new List<string> { "a" }));
        Assert.Equal(read.GetHashCode(), new RaftCommand.Read(1, new List<string> { "a" }).GetHashCode());
        Assert.NotEqual(read, new RaftCommand.Read(1, ["b"]));
        Assert.NotEqual(read, new RaftCommand.Read(2, ["a"]));
        Assert.False(read.Equals((RaftCommand.Read?)null));
        Assert.Equal((ulong?)1, read.CommandId);
        Assert.Null(new RaftCommand.Noop().CommandId);

        var exchange = Exchange(3);
        Assert.Equal((ulong?)3, exchange.CommandId);
        Assert.Equal(exchange, new RaftCommand.CompareExchange(3, new List<string> { "a" }, [new JournalVersion(0, new List<string>())], [new List<string> { "x" }]));
        Assert.Equal(exchange.GetHashCode(), Exchange(3).GetHashCode());
        Assert.False(exchange.Equals((RaftCommand.CompareExchange?)null));
        Assert.NotEqual(exchange, Exchange(4));
        Assert.NotEqual(exchange, new RaftCommand.CompareExchange(3, ["b"], [Empty], [new[] { "x" }]));
        Assert.NotEqual(exchange, new RaftCommand.CompareExchange(3, ["a"], [new JournalVersion(1, [])], [new[] { "x" }]));
        Assert.NotEqual(exchange, new RaftCommand.CompareExchange(3, ["a"], [new JournalVersion(0, ["y"])], [new[] { "x" }]));
        Assert.NotEqual(exchange, new RaftCommand.CompareExchange(3, ["a"], [Empty, Empty], [new[] { "x" }]));
        Assert.NotEqual(exchange, new RaftCommand.CompareExchange(3, ["a"], [Empty], [null]));
        Assert.NotEqual(new RaftCommand.CompareExchange(3, ["a"], [Empty], [null]), exchange);
        Assert.Equal(new RaftCommand.CompareExchange(3, ["a"], [Empty], [null]), new RaftCommand.CompareExchange(3, ["a"], [Empty], [null]));
        Assert.NotEqual(exchange, new RaftCommand.CompareExchange(3, ["a"], [Empty], [new[] { "y" }]));
        Assert.NotEqual(exchange, new RaftCommand.CompareExchange(3, ["a"], [Empty], [new[] { "x" }, null]));
        Assert.NotEqual<RaftCommand>(exchange, read);

        var outcome = new RaftOutcome.Read([new JournalVersion(1, ["x"])]);
        Assert.Equal(outcome, new RaftOutcome.Read(new List<JournalVersion> { new(1, new List<string> { "x" }) }));
        Assert.Equal(outcome.GetHashCode(), new RaftOutcome.Read([new JournalVersion(1, ["x"])]).GetHashCode());
        Assert.False(outcome.Equals((RaftOutcome.Read?)null));
        Assert.NotEqual(outcome, new RaftOutcome.Read([new JournalVersion(1, ["y"])]));
        Assert.NotEqual<RaftOutcome>(outcome, new RaftOutcome.Exchanged(true));

        var entries = new RaftEntry[] { new(1, read) };
        var append = new RaftMessage.Append(1, 0, 0, entries, 0);
        Assert.Equal(append, new RaftMessage.Append(1, 0, 0, entries.ToList(), 0));
        Assert.Equal(append.GetHashCode(), new RaftMessage.Append(1, 0, 0, entries.ToList(), 0).GetHashCode());
        Assert.False(append.Equals((RaftMessage.Append?)null));
        Assert.NotEqual(append, new RaftMessage.Append(1, 0, 0, entries, 1));
        Assert.NotEqual(append, new RaftMessage.Append(1, 0, 0, [], 0));
        Assert.NotEqual(append, new RaftMessage.Append(1, 0, 0, [new RaftEntry(2, read)], 0));
    }
}

internal static class RaftNodeTestExtensions
{
    public static ulong LastLogIndex(this RaftNode node) => (ulong)node.Log.Count - 1;
}
