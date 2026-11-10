using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineGroupCommitTests
{
    private sealed class ProbeLog : ICommitLog
    {
        public readonly List<string> Entries = new();
        public readonly List<string> Stable = new();
        public int Flushes;
        public int Appends;
        public string? Phase;
        public int Index = 1;
        public bool After, WriteThrough;
        public bool SupportsDurableFlush => true;
        public IReadOnlyList<string> Read() => Entries.ToArray();
        public void Append(string record)
        {
            bool fail = Phase == "append" && ++Appends == Index;
            if (fail && !After) throw new IOException("before append");
            Entries.Add(record);
            if (WriteThrough) { Stable.Clear(); Stable.AddRange(Entries); }
            if (fail) throw new IOException("after append");
        }
        public void Flush()
        {
            Flushes++;
            if (Phase == "flush" && !After) throw new IOException("before flush");
            Stable.Clear(); Stable.AddRange(Entries);
            if (Phase == "flush") throw new IOException("after flush");
        }
    }

    [Fact]
    public void IndependentSessionsKeepSeparateRevisionsWithOneFlush()
    {
        var log = new ProbeLog();
        var engine = new LinksEngine<uint>(log, new() { Isolation = IsolationLevel.Snapshot, HistoryLimit = null });
        using var first = engine.Begin();
        uint a = first.CreateLink(0, 0); first.SetName(a, "first");
        using var second = engine.Begin();
        uint b = second.CreateLink(a, 0); second.SetName(b, "second");
        Assert.Equal(0ul, engine.Snapshot().Revision);
        Assert.Equal(0, log.Flushes);
        Assert.Equal(new[] { new GroupCommitOutcome(GroupCommitStatus.Committed, 1), new GroupCommitOutcome(GroupCommitStatus.Committed, 2) }, engine.CommitGroup(new[] { first, second }));
        Assert.Equal(1, log.Flushes);
        Assert.Equal(2, log.Entries.Count);
        Assert.Single(engine.SnapshotAt(1).Rows);
        Assert.Equal("second", engine.SnapshotAt(2).Rows[b].Name);
        Assert.Equal("first", first.GetName(a));
        Assert.Equal("second", second.GetName(b));
    }

    [Fact]
    public void SynchronousReplicationFlushesEachCopyOnceAndExplicitCatchupStillFlushes()
    {
        var primary = new ProbeLog(); var follower = new ProbeLog();
        var replication = new ReplicatedCommitLog(primary, new[] { follower }, 2);
        var engine = new LinksEngine<uint>(replication, new() { Isolation = IsolationLevel.Snapshot });
        using var a = engine.Begin(); a.CreateLink(0, 0);
        using var b = engine.Begin(); b.CreateLink(1, 0);
        Assert.All(engine.CommitGroup(new[] { a, b }), outcome => Assert.Equal(GroupCommitStatus.Committed, outcome.Status));
        Assert.Equal(1, primary.Flushes); Assert.Equal(1, follower.Flushes);
        Assert.Equal(primary.Entries, follower.Entries);
        Assert.Equal(2, replication.Synchronize());
        Assert.Equal(2, primary.Flushes); Assert.Equal(2, follower.Flushes);
        Assert.Equal(2, follower.Entries.Count);
    }

    [Fact]
    public void EmptyReadOnlyClosedAndInvalidGroupsDoNoJournalIo()
    {
        var log = new ProbeLog();
        var engine = new LinksEngine<uint>(log);
        Assert.Empty(engine.CommitGroup(Array.Empty<EngineSession<uint>>()));
        using var reader = engine.Begin();
        using var closed = engine.Begin(); closed.Rollback();
        var results = engine.CommitGroup(new[] { reader, closed });
        Assert.Equal(new(GroupCommitStatus.Committed, 0), results[0]);
        Assert.Equal(GroupCommitStatus.Aborted, results[1].Status);
        Assert.Contains("closed", results[1].Error);
        using var local = engine.Begin(); local.CreateLink(0, 0);
        var other = new LinksEngine<uint>(new ProbeLog());
        using var foreign = other.Begin();
        Assert.Throws<ArgumentException>(() => engine.CommitGroup(new[] { local, foreign }));
        Assert.Throws<ArgumentException>(() => engine.CommitGroup(new[] { local, local }));
        Assert.Throws<ArgumentException>(() => engine.CommitGroup(new[] { local, null! }));
        Assert.Throws<ArgumentNullException>(() => engine.CommitGroup(null!));
        Assert.Equal(0, log.Flushes); Assert.Empty(log.Entries);
        // Invalid groups do not consume handles in the managed API.
        Assert.Equal(1ul, local.Commit()); foreign.CreateLink(0, 0); foreign.Commit();
    }

    [Fact]
    public void EntirelyAbortedWritesDoNotFlushAndFollowingGroupCanCommit()
    {
        var log = new ProbeLog();
        var engine = new LinksEngine<uint>(log, new() { Constraints = new[] { EngineConstraint.References } });
        using var bad = engine.Begin(); bad.CreateLink(999, 0);
        Assert.Equal(GroupCommitStatus.Aborted, Assert.Single(engine.CommitGroup(new[] { bad })).Status);
        Assert.Equal(0, log.Flushes); Assert.Empty(log.Entries);
        using var good = engine.Begin(); Assert.Equal(2u, good.CreateLink(0, 0));
        Assert.Equal(new(GroupCommitStatus.Committed, 1), Assert.Single(engine.CommitGroup(new[] { good })));
        Assert.Equal(1, log.Flushes);
    }

    [Theory]
    [InlineData(IsolationLevel.ReadCommitted, SerializableValidation.Revision)]
    [InlineData(IsolationLevel.Snapshot, SerializableValidation.Revision)]
    [InlineData(IsolationLevel.Serializable, SerializableValidation.Revision)]
    [InlineData(IsolationLevel.Serializable, SerializableValidation.ReadSet)]
    public void IsolationAndConflictsMatchIndividualCommits(IsolationLevel isolation, SerializableValidation validation)
    {
        var log = new ProbeLog();
        var engine = new LinksEngine<uint>(log, new() { Isolation = isolation, SerializableValidation = validation });
        using (var seed = engine.Begin()) { seed.EnsureCreated(1); seed.EnsureCreated(2); seed.Commit(); }
        using var first = engine.Begin(); using var second = engine.Begin(); using var same = engine.Begin();
        first.SetName(1, "a"); second.SetName(2, "b"); same.SetName(1, "stale");
        var outcomes = engine.CommitGroup(new[] { first, second, same });
        Assert.Equal(new(GroupCommitStatus.Committed, 2), outcomes[0]);
        bool disjoint = isolation != IsolationLevel.Serializable || validation == SerializableValidation.ReadSet;
        Assert.Equal(disjoint, outcomes[1].Status == GroupCommitStatus.Committed);
        Assert.Equal(GroupCommitStatus.Aborted, outcomes[2].Status);
        Assert.Equal("a", engine.Snapshot().Rows[1].Name);
        Assert.Equal(2, log.Flushes);
        using var following = engine.Begin(); following.CreateLink(0, 0); following.Commit();
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void ReadsAndScansValidateAgainstTentativePrefix(bool scan)
    {
        var engine = new LinksEngine<uint>(new ProbeLog(), new() { SerializableValidation = SerializableValidation.ReadSet });
        using (var seed = engine.Begin()) { seed.EnsureCreated(1); seed.Commit(); }
        using var writer = engine.Begin(); using var reader = engine.Begin();
        if (scan) Assert.Equal(1u, reader.Count(null)); else Assert.NotNull(reader.GetLink(1));
        writer.SetName(1, "changed");
        var outcomes = engine.CommitGroup(new[] { writer, reader });
        Assert.Equal(new(GroupCommitStatus.Committed, 2), outcomes[0]);
        Assert.Equal(GroupCommitStatus.Aborted, outcomes[1].Status);
        Assert.Contains(scan ? "revision" : "read", outcomes[1].Error);
    }

    [Theory]
    [InlineData(EngineConstraint.References)]
    [InlineData(EngineConstraint.UniqueNames)]
    [InlineData(EngineConstraint.UniquePairs)]
    public void ConstraintAbortsDoNotBurnRevisionsOrPreventLaterMembers(EngineConstraint constraint)
    {
        var log = new ProbeLog();
        var options = new EngineOptions { Isolation = IsolationLevel.Snapshot, Constraints = new[] { constraint }, HistoryLimit = null };
        var engine = new LinksEngine<uint>(log, options);
        using var first = engine.Begin(); uint a = first.CreateLink(0, 0); first.SetName(a, "same");
        using var bad = engine.Begin(); uint b = bad.CreateLink(constraint == EngineConstraint.References ? 999u : 0, 0); bad.SetName(b, "same");
        using var last = engine.Begin(); uint c = last.CreateLink(a, 0); last.SetName(c, "last");
        var outcomes = engine.CommitGroup(new[] { first, bad, last });
        Assert.Equal(new(GroupCommitStatus.Committed, 1), outcomes[0]);
        Assert.Equal(GroupCommitStatus.Aborted, outcomes[1].Status);
        Assert.Equal(new(GroupCommitStatus.Committed, 2), outcomes[2]);
        Assert.False(engine.Snapshot().Rows.ContainsKey(b));
        var reopened = new LinksEngine<uint>(log, options);
        Assert.Equal(engine.Snapshot().Rows, reopened.Snapshot().Rows);
        using var next = reopened.Begin(); Assert.Equal(4u, next.CreateLink(a, a)); next.Commit();
    }

    [Theory]
    [InlineData(ConcurrencyControl.Exclusive)]
    [InlineData(ConcurrencyControl.Pessimistic)]
    public void AdmissionPointOwnershipAndRetentionSurviveGroups(ConcurrencyControl concurrency)
    {
        var engine = new LinksEngine<uint>(new ProbeLog(), new() { Concurrency = concurrency, Isolation = IsolationLevel.Snapshot, HistoryLimit = 1 });
        using (var seed = engine.Begin()) { seed.EnsureCreated(1); seed.Commit(); }
        var pinned = engine.Snapshot();
        using var first = engine.Begin(); first.SetName(1, "first");
        var members = new List<EngineSession<uint>> { first };
        if (concurrency == ConcurrencyControl.Pessimistic)
        {
            var second = engine.Begin();
            Assert.Throws<InvalidOperationException>(() => second.SetName(1, "blocked"));
            second.CreateLink(1, 0); members.Add(second);
        }
        Assert.All(engine.CommitGroup(members), outcome => Assert.Equal(GroupCommitStatus.Committed, outcome.Status));
        foreach (var member in members) { member.Rollback(); member.Dispose(); Assert.Throws<InvalidOperationException>(() => member.Commit()); }
        Assert.Throws<KeyNotFoundException>(() => engine.SnapshotAt(1)); Assert.Single(pinned.Rows);
        using var following = engine.Begin(); following.SetName(1, "next"); following.Commit();
    }

    [Fact]
    public void UncertainIoPreservesKnownAbortsAndReopensTheDurablePrefix()
    {
        foreach (string phase in new[] { "append", "flush" })
            for (int index = 1; index <= (phase == "append" ? 2 : 1); index++)
                foreach (bool after in new[] { false, true })
                    foreach (bool writeThrough in new[] { false, true })
                    {
                        var log = new ProbeLog { Phase = phase, Index = index, After = after, WriteThrough = writeThrough };
                        var options = new EngineOptions { Isolation = IsolationLevel.ReadCommitted, Constraints = new[] { EngineConstraint.References }, RequireDurableFlush = true };
                        var engine = new LinksEngine<uint>(log, options);
                        using var observer = engine.Begin();
                        using var first = engine.Begin(); uint a = first.CreateLink(0, 0); first.SetName(a, "first");
                        using var bad = engine.Begin(); bad.CreateLink(999, 0);
                        using var last = engine.Begin(); last.CreateLink(a, 0);
                        using var reader = engine.Begin();
                        var outcomes = engine.CommitGroup(new[] { first, bad, last, reader });
                        Assert.Equal(GroupCommitStatus.Aborted, outcomes[1].Status);
                        foreach (int i in new[] { 0, 2, 3 })
                        {
                            Assert.Equal(GroupCommitStatus.Indeterminate, outcomes[i].Status);
                            Assert.Null(outcomes[i].Revision); Assert.Contains("reopen", outcomes[i].Error);
                        }
                        Assert.Equal(0u, observer.Count(null)); Assert.Null(first.GetName(a)); Assert.Null(last.GetLink(3));
                        Assert.Throws<InvalidOperationException>(() => engine.Snapshot());
                        Assert.Throws<InvalidOperationException>(() => engine.Begin());
                        Assert.Throws<InvalidOperationException>(() => engine.CommitGroup(Array.Empty<EngineSession<uint>>()));
                        int expected = phase == "flush" && after ? 2 : writeThrough ? (phase == "flush" ? 2 : index - 1 + (after ? 1 : 0)) : 0;
                        Assert.Equal(expected, log.Stable.Count);
                        var image = new MemoryCommitLog(); foreach (var record in log.Stable) image.Append(record);
                        var reopened = new LinksEngine<uint>(image, options with { RequireDurableFlush = false });
                        Assert.Equal((ulong)expected, reopened.Snapshot().Revision);
                        Assert.Equal(expected, reopened.Snapshot().Rows.Count);
                        if (expected > 0) Assert.Equal("first", reopened.Snapshot().Rows[a].Name);
                        using var next = reopened.Begin(); next.CreateLink(0, 0); Assert.Equal((ulong)expected + 1, next.Commit());
                        first.Dispose(); last.Rollback(); last.Rollback();
                    }
    }

    [Fact]
    public void TraceFailureDoesNotChangeAcknowledgementAndPoisonedGroupsReleaseAdmission()
    {
        var log = new ProbeLog();
        var engine = new LinksEngine<uint>(log, new() { Concurrency = ConcurrencyControl.Exclusive, Trace = _ => throw new IOException("trace") });
        using var first = engine.Begin(); first.CreateLink(0, 0);
        Assert.Equal(new(GroupCommitStatus.Committed, 1), Assert.Single(engine.CommitGroup(new[] { first })));
        using var held = engine.Begin();
        Assert.Throws<IOException>(() => engine.WithLog<int>(_ => throw new IOException("unknown")));
        Assert.Throws<InvalidOperationException>(() => engine.CommitGroup(new[] { held }));
        Assert.Equal(0, engine.Active);
    }

    [Theory]
    [InlineData(false, false)]
    [InlineData(true, false)]
    [InlineData(false, true)]
    [InlineData(true, true)]
    public void InterveningAbaCommitsRemainConflictsForGroupMembers(bool readDependency, bool grouped)
    {
        var log = new ProbeLog();
        var engine = new LinksEngine<uint>(log, new()
        {
            Isolation = readDependency ? IsolationLevel.Serializable : IsolationLevel.Snapshot,
            SerializableValidation = readDependency ? SerializableValidation.ReadSet : SerializableValidation.Revision
        });
        using (var seed = engine.Begin()) { seed.EnsureCreated(1); seed.Commit(); }
        using var stale = engine.Begin();
        if (readDependency) { Assert.Null(stale.GetName(1)); stale.CreateLink(0, 0); }
        else stale.SetName(1, "stale");
        void Complete(EngineSession<uint> session)
        {
            if (grouped) Assert.Equal(GroupCommitStatus.Committed, Assert.Single(engine.CommitGroup(new[] { session })).Status);
            else session.Commit();
        }
        using (var intervening = engine.Begin()) { intervening.SetName(1, "temporary"); Complete(intervening); }
        using (var intervening = engine.Begin()) { intervening.RemoveName(1); Complete(intervening); }
        using var valid = engine.Begin(); valid.CreateLink(0, 0);
        var outcomes = engine.CommitGroup(new[] { stale, valid });
        Assert.Equal(GroupCommitStatus.Aborted, outcomes[0].Status);
        Assert.Contains(readDependency ? "read" : "written", outcomes[0].Error);
        Assert.Equal(new(GroupCommitStatus.Committed, 4), outcomes[1]);
        Assert.Null(engine.Snapshot().Rows[1].Name);
        Assert.Equal(4, log.Flushes);
    }

    [Theory]
    [InlineData(ConcurrencyControl.Exclusive)]
    [InlineData(ConcurrencyControl.Pessimistic)]
    public void UncertainLastMembersReleaseAdmissionAndPointOwnership(ConcurrencyControl concurrency)
    {
        var engine = new LinksEngine<uint>(new ProbeLog { Phase = "append" }, new() { Concurrency = concurrency });
        using var member = engine.Begin(); member.CreateLink(0, 0);
        Assert.Equal(GroupCommitStatus.Indeterminate, Assert.Single(engine.CommitGroup(new[] { member })).Status);
        Assert.Equal(0, engine.Active); Assert.Null(engine.WriteOwners); Assert.Null(engine.Changed);
        member.Rollback(); member.Dispose();
        Assert.Equal(0, engine.Active);
        Assert.Throws<InvalidOperationException>(() => engine.Begin());
    }
}
