using System.Numerics;
using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineFailureTests
{
    [Theory]
    [InlineData(EngineConstraint.References)]
    [InlineData(EngineConstraint.UniqueNames)]
    [InlineData(EngineConstraint.UniquePairs)]
    public void ConstraintsRejectWholeTransaction(EngineConstraint constraint)
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog(), new() { Constraints = new[] { constraint } });
        using var tx = engine.Begin();
        uint first = tx.CreateLink(0, 0), second = tx.CreateLink(0, 0);
        if (constraint == EngineConstraint.References) tx.UpdateLink(first, 999, 0);
        if (constraint == EngineConstraint.UniqueNames) { tx.SetName(first, "same"); tx.SetName(second, "same"); }
        Assert.Throws<InvalidOperationException>(() => tx.Commit());
        Assert.Empty(engine.Snapshot().Rows);
    }
    [Fact]
    public void ConstraintsAllowInternallyConsistentWriteSets()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog(), new() { Constraints = Enum.GetValues<EngineConstraint>() });
        using var tx = engine.Begin();
        uint id = tx.CreateLink(0, 0); tx.UpdateLink(id, id, id); tx.SetName(id, "self"); tx.Commit();
        using var deletion = engine.Begin(); deletion.DeleteLink(id); deletion.Commit();
        Assert.Empty(engine.Snapshot().Rows);
    }
    [Fact]
    public void FailedAppendHasNoVisiblePartialState()
    {
        var log = new TestLog { FailAppend = true };
        var engine = new LinksEngine<uint>(log);
        using var tx = engine.Begin(); tx.CreateLink(0, 0);
        Assert.Throws<IOException>(() => tx.Commit());
        Assert.Throws<InvalidOperationException>(() => engine.Snapshot());
        Assert.Throws<InvalidOperationException>(() => engine.WithLog(log => log.Read()));
        log.FailAppend = false;
        Assert.Empty(new LinksEngine<uint>(log).Snapshot().Rows);
    }
    [Fact]
    public void AsyncReplicationCatchesUpWithoutDuplicates()
    {
        var primary = new MemoryCommitLog(); var follower = new TestLog();
        var replication = new ReplicatedCommitLog(primary, new[] { follower });
        var engine = new LinksEngine<uint>(replication);
        using var tx = engine.Begin(); tx.CreateLink(0, 0); tx.Commit();
        Assert.Empty(follower.Read());
        Assert.Equal(2, engine.WithLog(_ => replication.Synchronize()));
        Assert.Equal(2, engine.WithLog(_ => replication.Synchronize()));
        Assert.Single(follower.Read());
        Assert.Single(new LinksEngine<uint>(follower).Snapshot().Rows);
        follower.Append("divergence");
        Assert.Equal(1, replication.Synchronize());
    }
    [Fact]
    public void SyncReplicationPartitionHasUnknownOutcomeAndCanCatchUp()
    {
        var primary = new MemoryCommitLog(); var follower = new TestLog { FailFlush = true };
        var traced = new List<string>();
        var replication = new ReplicatedCommitLog(primary, new[] { follower }, 2) { Trace = traced.Add };
        var engine = new LinksEngine<uint>(replication);
        using var tx = engine.Begin(); tx.CreateLink(0, 0);
        Assert.Throws<IOException>(() => tx.Commit());
        Assert.NotEmpty(traced);
        follower.FailFlush = false;
        Assert.Equal(2, replication.Synchronize());
        Assert.Single(new LinksEngine<uint>(primary).Snapshot().Rows);
        Assert.Single(follower.Read());
    }
    [Fact]
    public void PrimaryBehindItsReplicaIsRefusedUntilRepaired()
    {
        var primary = new MemoryCommitLog(); var follower = new TestLog();
        var engine = new LinksEngine<uint>(new ReplicatedCommitLog(primary, new[] { follower }, 2));
        for (int i = 0; i < 2; i++) { using var tx = engine.Begin(); tx.CreateLink(0, 0); tx.Commit(); }
        // The primary loses its last acknowledged commit; the replica still has it.
        var survivor = new MemoryCommitLog(); survivor.Append(primary.Read()[0]);
        var refused = Assert.ThrowsAny<Exception>(() => new LinksEngine<uint>(new ReplicatedCommitLog(survivor, new[] { follower }, 2)));
        Assert.Contains("Replica 0 holds 1 records the primary lacks", refused.Message);
        Assert.Single(survivor.Read());
        // An unreadable or divergent replica proves nothing about the primary.
        var traced = new List<string>();
        var unreadable = new TestLog { FailRead = true };
        Assert.Single(new ReplicatedCommitLog(survivor, new[] { unreadable }) { Trace = traced.Add }.Read());
        Assert.Equal(new[] { "read failure" }, traced);
        var divergent = new TestLog(); divergent.Append("divergence"); divergent.Append(follower.Read()[1]);
        Assert.Single(new ReplicatedCommitLog(survivor, new[] { divergent }).Read());
        // Repaired from the replica, the primary opens with every commit.
        survivor.Append(follower.Read()[1]);
        Assert.Equal(2, new LinksEngine<uint>(new ReplicatedCommitLog(survivor, new[] { follower }, 2)).Snapshot().Rows.Count);
    }
    [Fact]
    public void InvalidStrategiesAndDurabilityClaimsFailEarly()
    {
        var log = new MemoryCommitLog();
        Assert.False(log.SupportsDurableFlush);
        foreach (var options in new EngineOptions[] {
            new() { HistoryLimit = -1 }, new() { Isolation = (IsolationLevel)99 },
            new() { Concurrency = (ConcurrencyControl)99 }, new() { Constraints = new[] { (EngineConstraint)99 } },
            new() { RequireDurableFlush = true } })
            Assert.Throws<ArgumentException>(() => new LinksEngine<uint>(log, options));
        Assert.Throws<ArgumentOutOfRangeException>(() => new ReplicatedCommitLog(log, Array.Empty<ICommitLog>(), 0));
        Assert.Throws<ArgumentOutOfRangeException>(() => new ReplicatedCommitLog(log, Array.Empty<ICommitLog>(), 2));
        Assert.Throws<ArgumentException>(() => new ReplicatedCommitLog(log, new[] { log }, 2));
        Assert.False(new ReplicatedCommitLog(log, new[] { new MemoryCommitLog() }, 2).SupportsDurableFlush);
        var durable = new TestLog { Durable = true };
        Assert.True(new ReplicatedCommitLog(durable, Array.Empty<ICommitLog>()).SupportsDurableFlush);
        Assert.True(new ReplicatedCommitLog(durable, new[] { log }, 1).SupportsDurableFlush);
        Assert.False(new ReplicatedCommitLog(durable, new[] { log }, 2).SupportsDurableFlush);
        Assert.True(new ReplicatedCommitLog(durable, new[] { new TestLog { Durable = true } }, 2).SupportsDurableFlush);
    }
    [Fact]
    public void PortableJournalVectorsMatchExactly()
    {
        using var vectors = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "engine-revision-vectors.json")));
        var valid = vectors.RootElement.GetProperty("valid").EnumerateArray().Select(value => value.GetString()!).ToArray();
        var log = new MemoryCommitLog();
        foreach (var record in valid) log.Append(record);
        var engine = new LinksEngine<uint>(log, new() { HistoryLimit = null });
        Assert.Equal("雪;|😀", engine.SnapshotAt(1).Rows[1].Name);
        Assert.Equal("", engine.Snapshot().Rows[uint.MaxValue].Name);
        foreach (var record in valid)
        {
            var (snapshot, next) = RevisionJournal<uint>.Decode(record);
            Assert.Equal(record, RevisionJournal<uint>.Encode(snapshot, next));
        }
        foreach (var record in vectors.RootElement.GetProperty("invalidU32").EnumerateArray())
            Assert.Throws<InvalidDataException>(() => RevisionJournal<uint>.Decode(record.GetString()!));
        var badOrder = new MemoryCommitLog(); badOrder.Append(valid[1]);
        Assert.Throws<InvalidDataException>(() => new LinksEngine<uint>(badOrder));
        var backwards = RevisionJournal<uint>.Encode(new EngineSnapshot<uint>(4, new()), BigInteger.One);
        log.Append(backwards);
        Assert.Throws<InvalidDataException>(() => new LinksEngine<uint>(log));
    }
    [Fact]
    public void ConcurrentSnapshotWritersPreserveEveryRow()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog(), new() { Isolation = IsolationLevel.Snapshot });
        Parallel.For(0, 4, _ =>
        {
            for (int i = 0; i < 16; i++)
            { using var tx = engine.Begin(); tx.CreateLink(0, 0); tx.Commit(); }
        });
        Assert.Equal(64, engine.Snapshot().Rows.Count);
    }
    [Fact]
    public void TraceFailuresCannotTurnCommittedDataIntoReportedFailures()
    {
        var log = new MemoryCommitLog();
        var engine = new LinksEngine<uint>(log, new() { Trace = _ => throw new InvalidOperationException("trace") });
        using var tx = engine.Begin(); tx.CreateLink(0, 0);
        Assert.Equal(1ul, tx.Commit());
        Assert.Single(new LinksEngine<uint>(log).Snapshot().Rows);
        var follower = new TestLog { FailFlush = true };
        var replication = new ReplicatedCommitLog(log, new[] { follower }) { Trace = _ => throw new IOException("trace") };
        Assert.Equal(1, replication.Synchronize());
    }
    private sealed class TestLog : ICommitLog
    {
        private readonly MemoryCommitLog _inner = new();
        public bool FailAppend, FailFlush, FailRead, Durable;
        public bool SupportsDurableFlush => Durable;
        public IReadOnlyList<string> Read() => FailRead ? throw new IOException("read failure") : _inner.Read();
        public void Append(string record) { if (FailAppend) throw new IOException("append failure"); _inner.Append(record); }
        public void Flush() { if (FailFlush) throw new IOException("partition"); }
    }
}
