using Foundation.Data.Doublets.Cli.Engine;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineStrategyTests
{
    [Fact]
    public void DirtyWritesAndRollbackRemainPrivate()
    {
        var engine = new LinksEngine<ulong>(new MemoryCommitLog());
        using var writer = engine.Begin();
        using var observer = engine.Begin();
        ulong id = writer.CreateLink(0, 0);
        writer.SetName(id, "雪;|😀");
        Assert.Null(observer.GetLink(id));
        writer.Rollback();
        Assert.Empty(engine.Snapshot().Rows);
        Assert.Throws<InvalidOperationException>(() => writer.CreateLink(0, 0));
    }
    [Theory]
    [InlineData(IsolationLevel.ReadCommitted, 1)]
    [InlineData(IsolationLevel.Snapshot, 0)]
    public void ReadVisibilityIsConfigurable(IsolationLevel isolation, int expected)
    {
        var engine = new LinksEngine<ulong>(new MemoryCommitLog(), new() { Isolation = isolation });
        using var observer = engine.Begin();
        using var writer = engine.Begin();
        writer.CreateLink(0, 0);
        writer.Commit();
        Assert.Equal((ulong)expected, observer.Count(null));
    }
    [Theory]
    [InlineData(IsolationLevel.Snapshot)]
    [InlineData(IsolationLevel.Serializable)]
    public void ConcurrentUpdatesConflict(IsolationLevel isolation)
    {
        var engine = new LinksEngine<ulong>(new MemoryCommitLog(), new() { Isolation = isolation });
        using (var seed = engine.Begin()) { seed.CreateLink(0, 0); seed.Commit(); }
        using var first = engine.Begin();
        using var second = engine.Begin();
        first.UpdateLink(1, 1, 0);
        second.UpdateLink(1, 0, 1);
        first.Commit();
        Assert.Throws<InvalidOperationException>(() => second.Commit());
        Assert.Equal(1ul, engine.Snapshot().Rows[1].Link.Source);
    }
    [Fact]
    public void SerializableScansPreventWriteSkew()
    {
        var engine = new LinksEngine<ulong>(new MemoryCommitLog());
        using var first = engine.Begin();
        using var second = engine.Begin();
        Assert.Equal(0ul, first.Count(null));
        Assert.Equal(0ul, second.Count(null));
        first.CreateLink(0, 0); second.CreateLink(0, 0);
        first.Commit();
        Assert.Throws<InvalidOperationException>(() => second.Commit());
    }
    [Fact]
    public void ExclusiveAdmissionAndPinnedHistory()
    {
        var engine = new LinksEngine<ulong>(new MemoryCommitLog(), new() { Concurrency = ConcurrencyControl.Exclusive, HistoryLimit = 2 });
        using (var held = engine.Begin()) Assert.Throws<InvalidOperationException>(() => engine.Begin());
        var pinned = engine.Snapshot();
        for (int i = 0; i < 3; i++) { using var tx = engine.Begin(); tx.CreateLink(0, 0); tx.Commit(); }
        Assert.Empty(pinned.Rows);
        Assert.Throws<KeyNotFoundException>(() => engine.SnapshotAt(1));
        Assert.Equal(2, engine.SnapshotAt(2).Rows.Count);
    }
    [Fact]
    public void StandardLinksApiWorksOnPrivateSessions()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var session = engine.Begin();
        var id = session.CreateAndUpdate(0u, 0u);
        session.SetName(id, "hello");
        Assert.Equal(id, session.GetByName("hello"));
        Assert.Equal(1u, session.Count(null));
        session.Commit();
        Assert.Single(engine.Snapshot().Rows);
    }
    [Fact]
    public void JournalRoundTripIncludesUnicodeAndDeletes()
    {
        var log = new MemoryCommitLog();
        var engine = new LinksEngine<ulong>(log, new() { HistoryLimit = null });
        using (var tx = engine.Begin()) { var id = tx.CreateLink(0, 0); tx.SetName(id, "雪;|😀"); tx.Commit(); }
        using (var tx = engine.Begin()) { tx.DeleteLink(1); tx.Commit(); }
        var reopened = new LinksEngine<ulong>(log, new() { HistoryLimit = null });
        Assert.Equal("雪;|😀", reopened.SnapshotAt(1).Rows[1].Name);
        Assert.Empty(reopened.Snapshot().Rows);
        Assert.Empty(reopened.SnapshotAt(0).Rows);
        log.Append("bad");
        Assert.Throws<InvalidDataException>(() => new LinksEngine<ulong>(log));
    }
    [Fact]
    public void FileLogRepairsTornTailAndRejectsSecondWriter()
    {
        var path = Path.Combine(Path.GetTempPath(), "engine-" + Guid.NewGuid().ToString("N"));
        try
        {
            using (var log = new FileCommitLog(path))
            {
                var engine = new LinksEngine<uint>(log, new() { RequireDurableFlush = true });
                using var tx = engine.Begin(); tx.CreateLink(0, 0); tx.Commit();
                Assert.Throws<IOException>(() => new FileCommitLog(path));
            }
            File.AppendAllText(path, "torn tail");
            using var reopened = new FileCommitLog(path);
            Assert.Single(new LinksEngine<uint>(reopened).Snapshot().Rows);
        }
        finally { File.Delete(path); File.Delete(path + ".lock"); }
    }
    [Fact]
    public void FlushFailurePoisonsHandleButReopenDiscoversCommit()
    {
        var log = new FaultLog();
        var engine = new LinksEngine<ulong>(log);
        using var tx = engine.Begin(); tx.CreateLink(0, 0);
        log.Fail = true;
        Assert.Throws<IOException>(() => tx.Commit());
        Assert.Throws<InvalidOperationException>(() => engine.Begin());
        log.Fail = false;
        Assert.Single(new LinksEngine<ulong>(log).Snapshot().Rows);
    }
    private sealed class FaultLog : ICommitLog
    {
        private readonly MemoryCommitLog _inner = new();
        public bool Fail;
        public bool SupportsDurableFlush => false;
        public IReadOnlyList<string> Read() => _inner.Read();
        public void Append(string record) => _inner.Append(record);
        public void Flush() { if (Fail) throw new IOException("Injected flush failure."); }
    }
}
