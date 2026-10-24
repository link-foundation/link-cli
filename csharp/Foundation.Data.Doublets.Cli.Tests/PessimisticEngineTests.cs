using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class PessimisticEngineTests
{
    private static EngineOptions Options => new() { Concurrency = ConcurrencyControl.Pessimistic, Isolation = IsolationLevel.Snapshot };
    private static LinksEngine<uint> Seeded(EngineOptions? options = null)
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog(), options ?? Options);
        using var seed = engine.Begin(); seed.EnsureCreated(1); seed.EnsureCreated(2); seed.Commit();
        return engine;
    }
    private static void Busy(Action operation) => Assert.Contains("busy", Assert.Throws<InvalidOperationException>(operation).Message, StringComparison.OrdinalIgnoreCase);

    [Fact]
    public void DisjointWritesCommitAndBusyOperationsPreservePriorStagedWrites()
    {
        var engine = Seeded(); var retained = engine.Snapshot();
        using var first = engine.Begin(); using var second = engine.Begin();
        first.UpdateLink(1, 1, 0); first.SetName(1, "first"); second.SetName(2, "second");
        Busy(() => second.UpdateLink(1, 0, 1)); Busy(() => second.DeleteLink(1));
        Busy(() => second.SetName(1, "busy")); Busy(() => second.RemoveName(1));
        Assert.Equal(0u, second.GetLink(1)!.Value.Source); Assert.Equal("second", second.GetName(2));
        Assert.Equal(1u, second.EnsureCreated(1)); // Visible existing row: no write.
        Assert.Equal(0u, engine.Snapshot().Rows[1].Link.Source);
        Assert.Equal(2ul, first.Commit()); Assert.Equal(3ul, second.Commit());
        Assert.Equal(0u, retained.Rows[1].Link.Source);
        Assert.Equal("first", engine.Snapshot().Rows[1].Name); Assert.Equal("second", engine.Snapshot().Rows[2].Name);
    }
    [Fact]
    public void AllocatedAndExplicitMissingAddressesShareOwnershipUntilRollback()
    {
        var engine = Seeded(); using var first = engine.Begin(); using var second = engine.Begin();
        var reserved = first.CreateLink(1, 2); var other = second.CreateLink(2, 1); Assert.NotEqual(reserved, other);
        Busy(() => second.EnsureCreated(reserved)); first.EnsureCreated(90); Busy(() => second.EnsureCreated(90));
        first.Rollback(); second.EnsureCreated(reserved); second.EnsureCreated(90); second.Commit();
        using var fresh = engine.Begin(); Assert.True(fresh.CreateLink(0, 0) > 90);
    }
    [Fact]
    public void ReverseKeyOrderFailsImmediatelyAndRollbackOrDisposeReleasesOwnership()
    {
        var engine = Seeded(); using var first = engine.Begin(); using var second = engine.Begin();
        first.SetName(1, "one"); second.SetName(2, "two");
        Busy(() => first.RemoveName(2)); Busy(() => second.RemoveName(1)); first.Commit(); second.Commit();
        foreach (bool dispose in new[] { false, true })
        {
            using var owner = engine.Begin(); using var retry = engine.Begin();
            owner.SetName(1, "private"); Busy(() => retry.RemoveName(1));
            if (dispose) owner.Dispose(); else owner.Rollback();
            retry.RemoveName(1); retry.Commit();
        }
    }
    [Theory]
    [InlineData(IsolationLevel.ReadCommitted)]
    [InlineData(IsolationLevel.Snapshot)]
    [InlineData(IsolationLevel.Serializable)]
    public void StaleWritersRejectBeforeStagingEvenWhenAbaRestoresTheRow(IsolationLevel isolation)
    {
        var engine = Seeded(Options with { Isolation = isolation }); using var stale = engine.Begin();
        var original = stale.GetLink(1);
        using (var deleted = engine.Begin()) { deleted.DeleteLink(1); deleted.Commit(); }
        using (var recreated = engine.Begin()) { recreated.EnsureCreated(1); recreated.Commit(); }
        Assert.Equal(original, engine.Snapshot().Rows[1].Link);
        Assert.Contains("conflict", Assert.Throws<InvalidOperationException>(() => stale.UpdateLink(1, 1, 1)).Message, StringComparison.OrdinalIgnoreCase);
        Assert.Equal(original, stale.GetLink(1));
        using var fresh = engine.Begin(); fresh.SetName(1, "fresh"); fresh.Commit(); stale.Rollback();
    }
    [Theory]
    [InlineData(SerializableValidation.Revision)]
    [InlineData(SerializableValidation.ReadSet)]
    public void SerializableValidationRemainsIndependentOfWriteOwnership(SerializableValidation validation)
    {
        var engine = Seeded(Options with { Isolation = IsolationLevel.Serializable, SerializableValidation = validation });
        using var first = engine.Begin(); using var second = engine.Begin();
        first.SetName(1, "one"); second.SetName(2, "two"); first.Commit();
        if (validation == SerializableValidation.ReadSet) second.Commit();
        else Assert.Throws<InvalidOperationException>(() => second.Commit());
        using var fresh = engine.Begin(); fresh.RemoveName(2); fresh.Commit();
    }
    [Fact]
    public void PointDependenciesAndScansStillInvalidateSerializableSessions()
    {
        var engine = Seeded(Options with { Isolation = IsolationLevel.Serializable, SerializableValidation = SerializableValidation.ReadSet });
        using var first = engine.Begin(); using var second = engine.Begin();
        first.GetName(2); second.GetName(1); first.SetName(1, "one"); second.SetName(2, "two"); first.Commit();
        Assert.Contains("read", Assert.Throws<InvalidOperationException>(() => second.Commit()).Message, StringComparison.OrdinalIgnoreCase);
        using var scan = engine.Begin(); scan.Count(null);
        using (var writer = engine.Begin()) { writer.SetName(2, "after"); writer.Commit(); }
        Assert.Throws<InvalidOperationException>(() => scan.Commit());
    }
    [Fact]
    public void BranchMergesRespectOwnedAddressesAndAllowDisjointPublication()
    {
        var engine = Seeded(); var branch = engine.Fork(new MemoryCommitLog());
        using var owner = engine.Begin(); owner.SetName(1, "private");
        using (var incoming = branch.Engine.Begin()) { incoming.SetName(2, "other"); incoming.Commit(); }
        branch.Merge();
        using (var incoming = branch.Engine.Begin()) { incoming.SetName(1, "branch"); incoming.Commit(); }
        Busy(() => branch.Merge()); Assert.Null(engine.Snapshot().Rows[1].Name);
        owner.Rollback(); branch.Merge();
        Assert.Equal("branch", engine.Snapshot().Rows[1].Name); Assert.Equal("other", engine.Snapshot().Rows[2].Name);
    }
    [Fact]
    public void ConstraintFailureReleasesOwnershipBeforeFreshRetryAndRecovery()
    {
        var engine = Seeded(Options with { Constraints = [EngineConstraint.UniqueNames] });
        using var first = engine.Begin(); using var rejected = engine.Begin(); using var retry = engine.Begin();
        first.SetName(1, "same"); rejected.SetName(2, "same"); Busy(() => retry.SetName(2, "retry")); first.Commit();
        Assert.Contains("constraint", Assert.Throws<InvalidOperationException>(() => rejected.Commit()).Message, StringComparison.OrdinalIgnoreCase);
        Assert.Throws<InvalidOperationException>(() => rejected.RemoveName(2));
        retry.SetName(2, "retry"); retry.Commit();
        var log = engine.WithLog(log => { var copy = new MemoryCommitLog(); foreach (var record in log.Read()) copy.Append(record); return copy; });
        var recovered = new LinksEngine<uint>(log, Options); Assert.Equal("retry", recovered.Snapshot().Rows[2].Name);
    }

    [Fact(Timeout = 30000)]
    public async Task FinitePessimisticHistoriesHaveAnIndependentSerialExecution()
    {
        await Task.Yield();
        int[][] orders = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
        for (int profile = 0; profile < 216; profile++)
        {
            int[] choices = [profile % 6, profile / 6 % 6, profile / 36];
            var reads = choices.Select(c => c / 2 + 1).ToArray();
            var writes = choices.Select(c => c % 2 + 1).ToArray();
            foreach (var order in orders)
            {
                var engine = Seeded(Options with { Isolation = IsolationLevel.Serializable, SerializableValidation = SerializableValidation.ReadSet });
                var sessions = Enumerable.Range(0, 3).Select(_ => engine.Begin()).ToArray();
                try
                {
                    bool[] admitted = [true, true, true];
                    for (int i = 0; i < 3; i++)
                    {
                        Assert.Null(sessions[i].GetName((uint)reads[i])); Assert.Null(sessions[i].GetName((uint)writes[i]));
                        try { sessions[i].SetName((uint)writes[i], i.ToString(System.Globalization.CultureInfo.InvariantCulture)); }
                        catch (InvalidOperationException error)
                        { Assert.Contains("busy", error.Message, StringComparison.OrdinalIgnoreCase); sessions[i].Rollback(); admitted[i] = false; }
                    }
                    bool[] accepted = new bool[3];
                    foreach (int i in order)
                    {
                        if (!admitted[i]) continue;
                        try { sessions[i].Commit(); accepted[i] = true; }
                        catch (InvalidOperationException error) { Assert.Contains("conflict", error.Message, StringComparison.OrdinalIgnoreCase); }
                    }
                    var actual = engine.Snapshot();
                    Assert.True(orders.Any(serial =>
                    {
                        string?[] names = [null, null];
                        foreach (int i in serial)
                        {
                            if (!accepted[i]) continue;
                            if ((reads[i] <= 2 && names[reads[i] - 1] is not null) || names[writes[i] - 1] is not null) return false;
                            names[writes[i] - 1] = i.ToString(System.Globalization.CultureInfo.InvariantCulture);
                        }
                        return names[0] == actual.Rows[1].Name && names[1] == actual.Rows[2].Name;
                    }), $"Nonserial profile={profile} order={string.Join(',', order)}");
                }
                finally { foreach (var session in sessions) session.Dispose(); }
            }
        }
    }

    private static object? PrivateField(object value, string name) => value.GetType()
        .GetField(name, System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.NonPublic)!.GetValue(value);
    private static int Owners(LinksEngine<uint> engine) => (PrivateField(engine, "WriteOwners") as System.Collections.IDictionary)?.Count ?? 0;

    [Fact]
    public void OwnershipIsSelectedAndReclaimedOnEveryTerminalPath()
    {
        foreach (var concurrency in new[] { ConcurrencyControl.Optimistic, ConcurrencyControl.Exclusive, ConcurrencyControl.Pessimistic })
        {
            var engine = Seeded(Options with { Concurrency = concurrency }); using var session = engine.Begin();
            Assert.Equal(concurrency == ConcurrencyControl.Pessimistic, PrivateField(session, "_locks") is not null);
            Assert.Equal(0, Owners(engine)); session.UpdateLink(1, 2, 3); session.SetName(1, "same key");
            Assert.Equal(concurrency == ConcurrencyControl.Pessimistic ? 1 : 0, Owners(engine)); session.Commit();
            Assert.Null(PrivateField(session, "_locks")); Assert.Equal(0, Owners(engine));
        }
        var poisoned = Seeded(); using var first = poisoned.Begin(); using var pending = poisoned.Begin();
        first.UpdateLink(1, 1, 0); pending.UpdateLink(2, 2, 0); Assert.Equal(2, Owners(poisoned));
        Assert.Throws<IOException>(() => poisoned.WithLog<int>(_ => throw new IOException("Unknown log outcome")));
        Assert.Throws<InvalidOperationException>(() => first.Commit()); Assert.Null(PrivateField(first, "_locks"));
        Assert.Equal(1, Owners(poisoned)); pending.Rollback(); Assert.Equal(0, Owners(poisoned));
        Assert.Equal(0, PrivateField(poisoned, "Active")); Assert.Null(PrivateField(poisoned, "Changed"));
    }

    private sealed class FaultLog : ICommitLog
    {
        private readonly MemoryCommitLog _inner = new();
        internal int Phase;
        public bool SupportsDurableFlush => false;
        public IReadOnlyList<string> Read() => _inner.Read();
        public void Append(string record)
        {
            if (Phase == 1) throw new IOException("Before append");
            _inner.Append(record);
            if (Phase == 2) throw new IOException("After append");
        }
        public void Flush() { if (Phase == 3) throw new IOException("Flush"); }
    }
    [Theory]
    [InlineData(1)]
    [InlineData(2)]
    [InlineData(3)]
    public void JournalFailuresReleaseOwnershipAndReopenDiscoversTheRecordedOutcome(int phase)
    {
        var log = new FaultLog(); var engine = new LinksEngine<uint>(log, Options);
        using (var seed = engine.Begin()) { seed.EnsureCreated(1); seed.EnsureCreated(2); seed.Commit(); }
        using var writer = engine.Begin(); using var pending = engine.Begin();
        writer.SetName(1, "uncertain"); pending.SetName(2, "private"); log.Phase = phase;
        Assert.Throws<IOException>(() => writer.Commit()); Assert.Equal(1, Owners(engine));
        Assert.Throws<InvalidOperationException>(() => writer.Commit()); Assert.Throws<InvalidOperationException>(() => writer.RemoveName(1));
        Assert.Throws<InvalidOperationException>(() => pending.Commit()); Assert.Equal(0, Owners(engine));
        log.Phase = 0; var recovered = new LinksEngine<uint>(log, Options);
        Assert.Equal(phase == 1 ? null : "uncertain", recovered.Snapshot().Rows[1].Name);
        Assert.Null(recovered.Snapshot().Rows[2].Name);
        using var retry = recovered.Begin(); retry.SetName(1, "retry"); retry.Commit();
    }
}
