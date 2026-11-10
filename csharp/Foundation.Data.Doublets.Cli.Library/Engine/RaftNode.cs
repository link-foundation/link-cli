namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Configuration of one <see cref="RaftNode"/>.</summary>
public sealed record RaftConfig
{
    public RaftConfig(int id, int members)
    {
        Id = id;
        Members = members;
        Seed = unchecked((ulong)id + 1);
    }
    /// <summary>This node's index in <c>0..Members</c>.</summary>
    public int Id { get; init; }
    public int Members { get; init; }
    /// <summary>The base election timeout in local clock units; each election
    /// waits a random time in <c>[ElectionTimeout, 2 * ElectionTimeout)</c>.</summary>
    public ulong ElectionTimeout { get; init; } = 100;
    public ulong HeartbeatInterval { get; init; } = 10;
    public bool PreVote { get; init; } = true;
    public bool CheckQuorum { get; init; } = true;
    /// <summary>Seeds the election jitter.</summary>
    public ulong Seed { get; init; }
    /// <summary>The most entries one append message carries.</summary>
    public int MaxEntriesPerMessage { get; init; } = 32;
}

/// <summary>A command in the replicated log.</summary>
public abstract record RaftCommand
{
    private RaftCommand() { }

    /// <summary>Starts every term: commits the entries of earlier terms.</summary>
    public sealed record Noop : RaftCommand;

    public sealed record Read(ulong Id, IReadOnlyList<string> Keys) : RaftCommand
    {
        public bool Equals(Read? other) => other is not null && Id == other.Id && Keys.SequenceEqual(other.Keys);
        public override int GetHashCode() => HashCode.Combine(Id, Keys.Count);
    }

    public sealed record CompareExchange(ulong Id, IReadOnlyList<string> Keys, IReadOnlyList<JournalVersion> Expected, IReadOnlyList<IReadOnlyList<string>?> Updates) : RaftCommand
    {
        public bool Equals(CompareExchange? other) =>
            other is not null && Id == other.Id && Keys.SequenceEqual(other.Keys) &&
            RaftEquality.Same(Expected, other.Expected) && RaftEquality.Same(Updates, other.Updates);
        public override int GetHashCode() => HashCode.Combine(Id, Keys.Count);
    }

    /// <summary>The id a caller waits on; a no-op has none.</summary>
    public ulong? CommandId => this switch
    {
        Read read => read.Id,
        CompareExchange exchange => exchange.Id,
        _ => null,
    };
}

public sealed record RaftEntry(ulong Term, RaftCommand Command);

/// <summary>What an applied command returned.</summary>
public abstract record RaftOutcome
{
    private RaftOutcome() { }

    public sealed record Read(IReadOnlyList<JournalVersion> Values) : RaftOutcome
    {
        public bool Equals(Read? other) => other is not null && RaftEquality.Same(Values, other.Values);
        public override int GetHashCode() => Values.Count;
    }

    public sealed record Exchanged(bool Applied) : RaftOutcome;
}

public abstract record RaftMessage
{
    private RaftMessage() { }

    public sealed record Vote(bool Pre, ulong Term, ulong LastIndex, ulong LastTerm) : RaftMessage;

    public sealed record VoteReply(bool Pre, ulong Term, bool Granted) : RaftMessage;

    public sealed record Append(ulong Term, ulong PrevIndex, ulong PrevTerm, IReadOnlyList<RaftEntry> Entries, ulong Commit) : RaftMessage
    {
        public bool Equals(Append? other) =>
            other is not null && (Term, PrevIndex, PrevTerm, Commit) == (other.Term, other.PrevIndex, other.PrevTerm, other.Commit) &&
            Entries.SequenceEqual(other.Entries);
        public override int GetHashCode() => HashCode.Combine(Term, PrevIndex, Entries.Count);
    }

    /// <summary><c>Index</c> is the last matching index on success and a hint otherwise.</summary>
    public sealed record AppendReply(ulong Term, bool Success, ulong Index) : RaftMessage;

    /// <summary>A follower forwards a proposal to the leader it knows.</summary>
    public sealed record Propose(RaftCommand Command) : RaftMessage;
}

public sealed record RaftEnvelope(int From, int To, RaftMessage Message);

public enum RaftRole
{
    Follower,
    PreCandidate,
    Candidate,
    Leader,
}

/// <summary>What must survive a restart: the term, the vote and the log.</summary>
public sealed record RaftHardState(ulong Term = 0, int? VotedFor = null);

/// <summary>The replicated journals, as <see cref="IAtomicJournalStore"/> exposes them.</summary>
public sealed class JournalState : IEquatable<JournalState>
{
    private readonly SortedDictionary<string, JournalVersion> _journals = new(StringComparer.Ordinal);
    private readonly List<(ulong Revision, ulong Command)> _exchanges = new();

    /// <summary>Bumped by every successful compare-exchange; never reused.</summary>
    public ulong Revision { get; private set; }
    public IReadOnlyDictionary<string, JournalVersion> Journals => _journals;
    /// <summary><c>(revision, command id)</c> of every successful compare-exchange.</summary>
    public IReadOnlyList<(ulong Revision, ulong Command)> Exchanges => _exchanges;

    private JournalVersion Get(string key) => _journals.GetValueOrDefault(key) ?? new(0, Array.Empty<string>());

    internal RaftOutcome? Apply(RaftCommand command)
    {
        switch (command)
        {
            case RaftCommand.Read read:
                return new RaftOutcome.Read(read.Keys.Select(Get).ToArray());
            case RaftCommand.CompareExchange exchange:
                var keys = exchange.Keys;
                if (keys.Count != exchange.Expected.Count || keys.Count != exchange.Updates.Count ||
                    keys.Where((key, i) => Get(key).Generation != exchange.Expected[i].Generation).Any())
                    return new RaftOutcome.Exchanged(false);
                Revision++;
                for (var i = 0; i < keys.Count; i++)
                    if (exchange.Updates[i] is { } entries) _journals[keys[i]] = new(Revision, entries.ToArray());
                _exchanges.Add((Revision, exchange.Id));
                return new RaftOutcome.Exchanged(true);
            default:
                return null;
        }
    }

    public bool Equals(JournalState? other) =>
        other is not null && Revision == other.Revision && _exchanges.SequenceEqual(other._exchanges) &&
        _journals.Count == other._journals.Count &&
        _journals.All(pair => other._journals.TryGetValue(pair.Key, out var version) && RaftEquality.Same(pair.Value, version));
    public override bool Equals(object? obj) => Equals(obj as JournalState);
    public override int GetHashCode() => HashCode.Combine(Revision, _journals.Count, _exchanges.Count);
}

/// <summary>Structural equality for the lists records compare by reference.</summary>
internal static class RaftEquality
{
    internal static bool Same(JournalVersion left, JournalVersion right) =>
        left.Generation == right.Generation && left.Entries.SequenceEqual(right.Entries);
    internal static bool Same(IReadOnlyList<JournalVersion> left, IReadOnlyList<JournalVersion> right) =>
        left.Count == right.Count && left.Zip(right).All(pair => Same(pair.First, pair.Second));
    internal static bool Same(IReadOnlyList<IReadOnlyList<string>?> left, IReadOnlyList<IReadOnlyList<string>?> right) =>
        left.Count == right.Count && left.Zip(right).All(pair => pair.First is null
            ? pair.Second is null
            : pair.Second is not null && pair.First.SequenceEqual(pair.Second));
}

/// <summary>A draft Raft replica that keeps the journals of an <see cref="IAtomicJournalStore"/>.
///
/// This is the algorithm etcd runs behind <see cref="EtcdJournalStore"/>, written as
/// a pure state machine in the style of etcd's <c>raft</c> package: the node reads no
/// clock, opens no socket and starts no thread. The caller passes the local time to
/// <see cref="Tick"/> and <see cref="Step"/>, delivers the messages from
/// <see cref="TakeMessages"/> over any transport, and persists <see cref="HardState"/>
/// and the log before sending them. That makes the same code usable in a real
/// deployment and in a deterministic simulation, where tests drive it with skewed and
/// jumping clocks, lost and reordered messages, partitions and restarts
/// (<c>EngineRaftSimulationTests.cs</c>).
///
/// Safety (election safety, log matching, leader completeness and state machine
/// safety) never depends on a clock: time only decides <em>when</em> a node starts an
/// election or a leader sends heartbeats. Clocks matter for liveness, and two etcd
/// options keep a cluster live under faults that a plain Raft does not survive:
/// <list type="bullet">
/// <item><b>PreVote</b> — a node first asks whether it <em>could</em> win before raising
/// its term, so a member that can send but not receive cannot disrupt a healthy leader
/// over and over.</item>
/// <item><b>CheckQuorum</b> — a leader that has not heard from a majority within an
/// election timeout steps down, and followers that heard from a leader within that
/// timeout refuse votes (leader stickiness). A leader that can send but not receive
/// therefore stops blocking an election.</item>
/// </list>
///
/// Reads are linearizable because they go through the log (as etcd's older quorum
/// reads did): a read returns the state at its position in the committed order. No
/// proposal is retried; a proposal lost on the way to the leader simply never commits,
/// and the caller's deadline reports an unknown outcome, as
/// <see cref="IAtomicJournalStore"/> requires.
///
/// This is a draft for users to try: membership changes, snapshots/log compaction and
/// ReadIndex/lease reads are not implemented. Mirrors rust/src/engine/raft.rs.</summary>
public sealed class RaftNode
{
    /// <summary>splitmix64, used only to randomize election timeouts.</summary>
    private sealed class Jitter(ulong state)
    {
        public ulong Below(ulong bound)
        {
            unchecked
            {
                state += 0x9E3779B97F4A7C15UL;
                var z = state;
                z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9UL;
                z = (z ^ (z >> 27)) * 0x94D049BB133111EBUL;
                return (z ^ (z >> 31)) % Math.Max(bound, 1);
            }
        }
    }

    private readonly RaftConfig _config;
    private readonly Jitter _jitter;
    // Persistent.
    private RaftHardState _hard = new();
    /// <summary><c>_log[0]</c> is a sentinel of term 0.</summary>
    private readonly List<RaftEntry> _log = [new(0, new RaftCommand.Noop())];
    // Volatile.
    private RaftRole _role = RaftRole.Follower;
    private int? _leader;
    private ulong _commit, _applied;
    private JournalState _state = new();
    private readonly SortedSet<int> _votes = new();
    private readonly ulong[] _next, _matched;
    /// <summary>Peers heard from since the last quorum check.</summary>
    private readonly SortedSet<int> _active = new();
    private ulong _electionDue, _heartbeatDue, _quorumDue;
    /// <summary>When this node last heard from a current leader.</summary>
    private ulong? _leaderHeard;
    private readonly HashSet<ulong> _waiting = new();
    private readonly Dictionary<ulong, RaftOutcome> _outcomes = new();
    private List<RaftEnvelope> _outbox = new();

    public RaftNode(RaftConfig config, ulong now)
    {
        if (config.Members <= 0 || config.Id < 0 || config.Id >= config.Members || config.MaxEntriesPerMessage < 0)
            throw new ArgumentOutOfRangeException(nameof(config), "The node id must be in 0..members and the entry limit non-negative.");
        if (config.ElectionTimeout <= config.HeartbeatInterval)
            throw new ArgumentException("The election timeout must exceed the heartbeat interval.", nameof(config));
        _config = config;
        _jitter = new(config.Seed);
        _next = Enumerable.Repeat(1UL, config.Members).ToArray();
        _matched = new ulong[config.Members];
        ResetElection(now);
    }

    /// <summary>A process restart: the hard state and the log survive, everything
    /// else (commit index, applied state, pending outcomes) is rebuilt.</summary>
    public void Restart(ulong now)
    {
        _role = RaftRole.Follower;
        _leader = null;
        _commit = 0;
        _applied = 0;
        _state = new();
        _votes.Clear();
        _active.Clear();
        _leaderHeard = null;
        _waiting.Clear();
        _outcomes.Clear();
        _outbox.Clear();
        ResetElection(now);
    }

    public int Id => _config.Id;
    public RaftRole Role => _role;
    public ulong Term => _hard.Term;
    public int? Leader => _leader;
    public RaftHardState HardState => _hard;
    public ulong CommitIndex => _commit;
    public ulong AppliedIndex => _applied;
    /// <summary>The whole log, sentinel included.</summary>
    public IReadOnlyList<RaftEntry> Log => _log;
    public JournalState State => _state;

    /// <summary>Messages to deliver; persist the hard state and log first.</summary>
    public List<RaftEnvelope> TakeMessages()
    {
        var messages = _outbox;
        _outbox = new();
        return messages;
    }

    /// <summary>The outcome of a command proposed on this node, once applied here.</summary>
    public RaftOutcome? TakeOutcome(ulong id) => _outcomes.Remove(id, out var outcome) ? outcome : null;

    /// <summary>Gives up waiting for a command (its caller timed out).</summary>
    public void Forget(ulong id)
    {
        _waiting.Remove(id);
        _outcomes.Remove(id);
    }

    /// <summary>Proposes a command. Returns false when no leader is known, so the
    /// proposal went nowhere. Even on true the proposal may be lost.</summary>
    public bool Propose(ulong now, RaftCommand command)
    {
        _ = now;
        if (command.CommandId is { } id) _waiting.Add(id);
        if (_role == RaftRole.Leader)
        {
            AppendLocal(command);
            BroadcastAppend();
            return true;
        }
        if (_leader is { } leader)
        {
            Send(leader, new RaftMessage.Propose(command));
            return true;
        }
        return false;
    }

    /// <summary>Advances timers to the local time <paramref name="now"/>. A clock
    /// that jumps backward only delays them.</summary>
    public void Tick(ulong now)
    {
        if (_role == RaftRole.Leader)
        {
            if (_config.CheckQuorum && now >= _quorumDue)
            {
                _quorumDue = now + _config.ElectionTimeout;
                var heard = _active.Count + 1;
                _active.Clear();
                if (heard < Quorum)
                {
                    BecomeFollower(now, _hard.Term, null);
                    return;
                }
            }
            if (now >= _heartbeatDue)
            {
                _heartbeatDue = now + _config.HeartbeatInterval;
                BroadcastAppend();
            }
        }
        else if (now >= _electionDue)
        {
            Campaign(now, _config.PreVote);
        }
    }

    public void Step(ulong now, RaftEnvelope envelope)
    {
        var from = envelope.From;
        if (from < 0 || from >= _config.Members || from == _config.Id) return;
        ulong? term = envelope.Message switch
        {
            RaftMessage.Vote vote => vote.Term,
            RaftMessage.VoteReply reply => reply.Term,
            RaftMessage.Append append => append.Term,
            RaftMessage.AppendReply reply => reply.Term,
            _ => null,
        };
        if (term > _hard.Term)
        {
            if (envelope.Message is RaftMessage.Vote && InLease(now))
            {
                // Leader stickiness: ignore disruptive campaigns.
                return;
            }
            var preRequest = envelope.Message is RaftMessage.Vote { Pre: true };
            var preGrant = envelope.Message is RaftMessage.VoteReply { Pre: true, Granted: true };
            if (!preRequest && !preGrant)
                BecomeFollower(now, term.Value, envelope.Message is RaftMessage.Append ? from : null);
        }
        else if (term < _hard.Term)
        {
            switch (envelope.Message)
            {
                // Tell a stale leader or candidate about the newer term.
                case RaftMessage.Append:
                    Send(from, new RaftMessage.AppendReply(_hard.Term, false, 0));
                    break;
                case RaftMessage.Vote vote:
                    Send(from, new RaftMessage.VoteReply(vote.Pre, _hard.Term, false));
                    break;
            }
            return;
        }
        switch (envelope.Message)
        {
            case RaftMessage.Vote vote:
                OnVote(now, from, vote.Pre, vote.Term, vote.LastIndex, vote.LastTerm);
                break;
            case RaftMessage.VoteReply reply:
                OnVoteReply(now, from, reply.Pre, reply.Term, reply.Granted);
                break;
            case RaftMessage.Append append:
                OnAppend(now, from, append.PrevIndex, append.PrevTerm, append.Entries, append.Commit);
                break;
            case RaftMessage.AppendReply reply:
                OnAppendReply(from, reply.Success, reply.Index);
                break;
            case RaftMessage.Propose propose:
                if (_role == RaftRole.Leader)
                {
                    AppendLocal(propose.Command);
                    BroadcastAppend();
                }
                // Otherwise the proposal is dropped: no proposal is retried.
                break;
        }
    }

    private int Quorum => _config.Members / 2 + 1;
    private ulong LastIndex => (ulong)_log.Count - 1;
    private ulong LastTerm => _log[^1].Term;

    private bool InLease(ulong now) =>
        _config.CheckQuorum && (_role == RaftRole.Leader ||
            (_leader is not null && _leaderHeard is { } heard && now < heard + _config.ElectionTimeout));

    private void Send(int to, RaftMessage message) => _outbox.Add(new(_config.Id, to, message));

    private void ResetElection(ulong now) =>
        _electionDue = now + _config.ElectionTimeout + _jitter.Below(_config.ElectionTimeout);

    private void BecomeFollower(ulong now, ulong term, int? leader)
    {
        if (term > _hard.Term) _hard = new(term, null);
        _role = RaftRole.Follower;
        _leader = leader;
        _votes.Clear();
        if (leader is not null) _leaderHeard = now;
        ResetElection(now);
    }

    private void Campaign(ulong now, bool pre)
    {
        ResetElection(now);
        _votes.Clear();
        _votes.Add(_config.Id);
        ulong term;
        if (pre)
        {
            _role = RaftRole.PreCandidate;
            term = _hard.Term + 1;
        }
        else
        {
            _role = RaftRole.Candidate;
            _hard = new(_hard.Term + 1, _config.Id);
            term = _hard.Term;
        }
        _leader = null;
        if (_votes.Count >= Quorum)
        {
            Win(now, pre);
            return;
        }
        var (lastIndex, lastTerm) = (LastIndex, LastTerm);
        for (var peer = 0; peer < _config.Members; peer++)
            if (peer != _config.Id) Send(peer, new RaftMessage.Vote(pre, term, lastIndex, lastTerm));
    }

    private void Win(ulong now, bool pre)
    {
        if (pre)
        {
            Campaign(now, false);
            return;
        }
        _role = RaftRole.Leader;
        _leader = _config.Id;
        _votes.Clear();
        _active.Clear();
        Array.Fill(_next, LastIndex + 1);
        Array.Fill(_matched, 0UL);
        _quorumDue = now + _config.ElectionTimeout;
        _heartbeatDue = now + _config.HeartbeatInterval;
        AppendLocal(new RaftCommand.Noop());
        BroadcastAppend();
    }

    private void OnVote(ulong now, int from, bool pre, ulong term, ulong lastIndex, ulong lastTerm)
    {
        var upToDate = lastTerm > LastTerm || (lastTerm == LastTerm && lastIndex >= LastIndex);
        var granted = pre
            // A pre-vote never changes this node's state.
            ? term > _hard.Term && upToDate && !InLease(now)
            : term == _hard.Term && upToDate && (_hard.VotedFor is null || _hard.VotedFor == from);
        if (granted && !pre)
        {
            _hard = _hard with { VotedFor = from };
            ResetElection(now);
        }
        Send(from, new RaftMessage.VoteReply(pre, pre && granted ? term : _hard.Term, granted));
    }

    private void OnVoteReply(ulong now, int from, bool pre, ulong term, bool granted)
    {
        ulong? expected = (_role, pre) switch
        {
            (RaftRole.PreCandidate, true) => _hard.Term + 1,
            (RaftRole.Candidate, false) => _hard.Term,
            _ => null,
        };
        if (term != expected || !granted) return;
        _votes.Add(from);
        if (_votes.Count >= Quorum) Win(now, pre);
    }

    private void OnAppend(ulong now, int from, ulong prevIndex, ulong prevTerm, IReadOnlyList<RaftEntry> entries, ulong commit)
    {
        // The term equals ours here; a candidate of this term steps down.
        if (_role != RaftRole.Follower || _leader != from)
        {
            _role = RaftRole.Follower;
            _leader = from;
            _votes.Clear();
        }
        _leaderHeard = now;
        ResetElection(now);
        if (prevIndex > LastIndex || _log[(int)prevIndex].Term != prevTerm)
        {
            // Saturating: prevIndex - 1, never below zero.
            var hint = Math.Min(Math.Max(prevIndex, 1) - 1, LastIndex);
            Send(from, new RaftMessage.AppendReply(_hard.Term, false, hint));
            return;
        }
        var index = prevIndex;
        foreach (var entry in entries)
        {
            index++;
            if (index <= LastIndex)
            {
                if (_log[(int)index].Term == entry.Term) continue;
                // A conflicting suffix was never committed: drop it.
                if (index <= _commit) throw new InvalidOperationException("A committed entry would be overwritten.");
                _log.RemoveRange((int)index, _log.Count - (int)index);
            }
            _log.Add(entry);
        }
        if (commit > _commit)
        {
            _commit = Math.Min(commit, index);
            Apply();
        }
        Send(from, new RaftMessage.AppendReply(_hard.Term, true, index));
    }

    private void OnAppendReply(int from, bool success, ulong index)
    {
        if (_role != RaftRole.Leader) return;
        _active.Add(from);
        if (success)
        {
            if (index > _matched[from]) _matched[from] = index;
            _next[from] = Math.Max(_next[from], index + 1);
            AdvanceCommit();
            if (_next[from] <= LastIndex) SendAppend(from);
        }
        else
        {
            // Saturating: next[from] - 1, never below zero.
            _next[from] = Math.Max(Math.Min(index + 1, Math.Max(_next[from], 1) - 1), 1);
            SendAppend(from);
        }
    }

    private void AppendLocal(RaftCommand command)
    {
        _log.Add(new(_hard.Term, command));
        var id = _config.Id;
        _matched[id] = LastIndex;
        _next[id] = LastIndex + 1;
        AdvanceCommit();
    }

    private void BroadcastAppend()
    {
        for (var peer = 0; peer < _config.Members; peer++)
            if (peer != _config.Id) SendAppend(peer);
    }

    private void SendAppend(int peer)
    {
        var prevIndex = _next[peer] - 1;
        var prevTerm = _log[(int)prevIndex].Term;
        var start = (int)prevIndex + 1;
        var end = (int)Math.Min(_log.Count, (long)start + _config.MaxEntriesPerMessage);
        var entries = _log.GetRange(start, end - start).ToArray();
        Send(peer, new RaftMessage.Append(_hard.Term, prevIndex, prevTerm, entries, _commit));
    }

    private void AdvanceCommit()
    {
        var matched = _matched.ToArray();
        Array.Sort(matched);
        Array.Reverse(matched);
        var candidate = matched[Quorum - 1];
        // Only entries of the current term commit by counting replicas.
        if (candidate > _commit && _log[(int)candidate].Term == _hard.Term)
        {
            _commit = candidate;
            Apply();
        }
    }

    private void Apply()
    {
        while (_applied < _commit)
        {
            _applied++;
            var command = _log[(int)_applied].Command;
            if (_state.Apply(command) is { } outcome)
            {
                // Only commands with ids have outcomes.
                var id = command.CommandId!.Value;
                if (_waiting.Remove(id)) _outcomes[id] = outcome;
            }
        }
    }
}
