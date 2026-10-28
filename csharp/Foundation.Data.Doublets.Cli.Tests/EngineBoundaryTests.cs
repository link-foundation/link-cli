using System.Text;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineBoundaryTests
{
    private static string Record(string payload)
    {
        ulong hash = 0xcbf29ce484222325;
        foreach (var value in Encoding.UTF8.GetBytes(payload)) hash = unchecked((hash ^ value) * 0x100000001b3);
        return $"{payload}|{hash:x16}";
    }
    private static MemoryCommitLog Log(params string[] payloads)
    {
        var log = new MemoryCommitLog();
        foreach (var payload in payloads) log.Append(Record(payload));
        return log;
    }

    [Fact]
    public void IndividuallyValidRecordsMustFormAContiguousMonotonicJournal()
    {
        foreach (var payloads in new string[][] {
            ["engine1|2|1|"], ["engine1|1|1|", "engine1|1|1|"],
            ["engine1|1|1|", "engine1|3|1|"], ["engine1|1|100|", "engine1|2|99|"] })
            Assert.Throws<InvalidDataException>(() => new LinksEngine<ulong>(Log(payloads)));
        var engine = new LinksEngine<ulong>(Log("engine1|1|100|", "engine1|2|100|"));
        Assert.Equal(2ul, engine.SnapshotAt(2).Revision);
        using var next = engine.Begin(); Assert.Equal(100ul, next.CreateLink(0, 0));
    }

    [Fact]
    public void EmptyFieldsAreRejectedEvenWithAValidChecksum()
    {
        foreach (var payload in new[] { "engine1||1|", "engine1|1||", "engine1|1|2|,0,0,-",
            "engine1|1|2|1,,0,-", "engine1|1|2|1,0,,-" })
            Assert.Throws<InvalidDataException>(() => new LinksEngine<ulong>(Log(payload)));
    }

    [Fact]
    public void SavedMergeBaseMustMatchRowsAndAllocatorAsWellAsRevision()
    {
        var parent = new LinksEngine<ulong>(new MemoryCommitLog());
        var log = Log("engine1|1|2|1,0,0,-");
        foreach (var payload in new[] { "engine1|1|2|1,1,0,-", "engine1|1|3|1,0,0,-" })
            Assert.Throws<InvalidDataException>(() => parent.ResumeBranch(log, Record(payload)));
        Assert.Single(parent.ResumeBranch(log, Record("engine1|1|2|1,0,0,-")).Engine.Snapshot().Rows);
    }

    [Fact]
    public void LostLogOperationReplyFencesPendingSessionsAndReopenDiscoversOutcome()
    {
        var log = new MemoryCommitLog(); var engine = new LinksEngine<ulong>(log);
        using var pending = engine.Begin(); pending.CreateLink(0, 0);
        Assert.Throws<IOException>(() => engine.WithLog<int>(journal =>
        {
            journal.Append(Record("engine1|1|2|1,0,0,-"));
            throw new IOException("injected lost diagnostic reply");
        }));
        Assert.Throws<InvalidOperationException>(() => pending.Commit());
        Assert.Throws<InvalidOperationException>(() => engine.Begin());
        Assert.Throws<InvalidOperationException>(() => engine.Snapshot());
        pending.Rollback();
        Assert.Single(new LinksEngine<ulong>(log).Snapshot().Rows);
    }

    [Fact]
    public void UInt128AllocatorExhaustionNeverWrapsOrPartiallyAdvances()
    {
        var log = new MemoryCommitLog(); var engine = new LinksEngine<UInt128>(log);
        using var tx = engine.Begin();
        Assert.Throws<OverflowException>(() => tx.EnsureCreated(UInt128.MaxValue));
        Assert.Empty(engine.Snapshot().Rows);
        Assert.Equal(UInt128.One, tx.CreateLink(0, 0));
        tx.EnsureCreated(UInt128.MaxValue - 1);
        Assert.Throws<OverflowException>(() => tx.CreateLink(0, 0));
        Assert.Throws<OverflowException>(() => tx.CreateLink(0, 0));
        Assert.Null(tx.GetLink(0)); tx.Commit();
        var reopened = new LinksEngine<UInt128>(log);
        Assert.Equal(2, reopened.Snapshot().Rows.Count);
        using var next = reopened.Begin(); Assert.Throws<OverflowException>(() => next.CreateLink(0, 0));
    }

    [Fact]
    public void RevisionExhaustionClosesWithoutAppendingOrPoisoning()
    {
        var log = new MemoryCommitLog(); var engine = new LinksEngine<ulong>(log);
        // Finite fixture: no loop up to the counter limit.
        engine.Current = new EngineSnapshot<ulong>(ulong.MaxValue, new());
        using var tx = engine.Begin(); tx.CreateLink(0, 0);
        Assert.Throws<OverflowException>(() => tx.Commit());
        Assert.Throws<InvalidOperationException>(() => tx.Commit());
        Assert.Empty(log.Read()); Assert.Empty(engine.Snapshot().Rows);
        using var reader = engine.Begin(); Assert.Equal(ulong.MaxValue, reader.Commit());
        Assert.Equal(0, engine.Active);
    }

    [Fact]
    public void ClosedReadCommittedSessionsKeepTheirBaseAfterRollback()
    {
        var engine = new LinksEngine<ulong>(new MemoryCommitLog(), new() { Isolation = IsolationLevel.ReadCommitted });
        using (var seed = engine.Begin()) { seed.CreateLink(0, 0); seed.Commit(); }
        using var reader = engine.Begin();
        using (var writer = engine.Begin()) { writer.SetName(1, "new"); writer.Commit(); }
        Assert.Equal("new", reader.GetName(1)); Assert.Equal(1ul, reader.Count(null));
        reader.Rollback(); Assert.Null(reader.GetName(1));
        using (var deleted = engine.Begin()) { deleted.DeleteLink(1); deleted.Commit(); }
        Assert.NotNull(reader.GetLink(1)); Assert.Equal(1ul, reader.Count(null));
        Assert.Empty(engine.Snapshot().Rows);
    }
}
