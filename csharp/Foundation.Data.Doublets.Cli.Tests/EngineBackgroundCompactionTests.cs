using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineBackgroundCompactionTests
{
    private static readonly TimeSpan Period = TimeSpan.FromMilliseconds(1);
    private static readonly TimeSpan Patience = TimeSpan.FromSeconds(30);

    private sealed class Log : IReplaceableCommitLog
    {
        private readonly object _gate = new();
        private string[] _rows = Array.Empty<string>();
        public bool Fail { get; init; }
        public bool SupportsReplacement { get; init; } = true;
        public bool SupportsDurableFlush => false;
        public IReadOnlyList<string> Read() { lock (_gate) return _rows.ToArray(); }
        public void Append(string row) { lock (_gate) _rows = _rows.Append(row).ToArray(); }
        public void Flush() { }
        public bool Replace(IReadOnlyList<string> rows)
        {
            if (Fail) throw new IOException("injected failure");
            lock (_gate)
            {
                var changed = !_rows.SequenceEqual(rows);
                _rows = rows.ToArray();
                return changed;
            }
        }
    }

    private static void Commit(LinksEngine<uint> engine)
    {
        using var tx = engine.Begin();
        tx.CreateLink(0, 0);
        tx.Commit();
    }

    private static void Until(Func<bool> done)
    {
        Assert.True(SpinWait.SpinUntil(done, Patience), "the worker made no progress");
    }

    private static bool Refuses(LinksEngine<uint> engine)
    {
        try { Commit(engine); return false; }
        catch (InvalidOperationException) { return true; }
    }

    [Fact]
    public void ThresholdCompactionWaitsForTheSelectedNumberOfCommits()
    {
        var engine = new LinksEngine<uint>(new Log());
        Assert.Throws<ArgumentOutOfRangeException>(() => engine.CompactJournalAfter(0));
        Commit(engine);
        Assert.False(engine.CompactJournalAfter(2));
        Commit(engine);
        Assert.True(engine.CompactJournalAfter(2));
        Assert.False(engine.CompactJournalAfter(1));
        Commit(engine);
        Assert.True(engine.CompactJournal());
        Assert.False(engine.CompactJournalAfter(1));
        Commit(engine);
        Assert.True(engine.CompactJournalAfter(1));
    }

    [Fact]
    public void ThresholdCompactionRefusesAnUnreplaceableLogBeforeAnythingIsDue()
    {
        var engine = new LinksEngine<uint>(new Log { SupportsReplacement = false });
        Assert.Throws<NotSupportedException>(() => engine.CompactJournalAfter(ulong.MaxValue));
    }

    [Fact]
    public void TheWorkerCompactsALongJournalAndTheStateSurvivesReopening()
    {
        var log = new Log();
        var engine = new LinksEngine<uint>(log);
        for (var i = 0; i < 4; i++) Commit(engine);
        var length = log.Read().Count;
        using (var worker = new BackgroundCompaction<uint>(engine, 2, Period))
        {
            Until(() => log.Read().Count < length);
            worker.Stop();
        }
        var snapshot = engine.Snapshot();
        var reopened = new LinksEngine<uint>(log).Snapshot();
        Assert.Equal(4UL, reopened.Revision);
        Assert.Equal(snapshot.Rows.Count, reopened.Rows.Count);
    }

    [Fact]
    public void CommitsContinueWhileTheWorkerRuns()
    {
        var log = new Log();
        var engine = new LinksEngine<uint>(log);
        using var worker = new BackgroundCompaction<uint>(engine, 1, Period);
        for (var i = 0; i < 64; i++) Commit(engine);
        worker.Stop();
        Assert.Equal(64UL, engine.Snapshot().Revision);
        Assert.Equal(64UL, new LinksEngine<uint>(log).Snapshot().Revision);
    }

    [Fact]
    public void AWrongSelectionIsRefusedBeforeAWorkerStarts()
    {
        var engine = new LinksEngine<uint>(new Log());
        Assert.Throws<ArgumentNullException>(() => new BackgroundCompaction<uint>(null!, 1, Period));
        Assert.Throws<ArgumentOutOfRangeException>(() => new BackgroundCompaction<uint>(engine, 0, Period));
        Assert.Throws<ArgumentOutOfRangeException>(() => new BackgroundCompaction<uint>(engine, 1, TimeSpan.Zero));
        var fixedLog = new LinksEngine<uint>(new Log { SupportsReplacement = false });
        Assert.Throws<NotSupportedException>(() => new BackgroundCompaction<uint>(fixedLog, 1, Period));
    }

    [Fact]
    public void AFailedPassEndsTheWorkerFencesTheEngineAndIsReported()
    {
        var engine = new LinksEngine<uint>(new Log { Fail = true });
        Commit(engine);
        using var worker = new BackgroundCompaction<uint>(engine, 1, Period);
        Until(() => Refuses(engine));
        Assert.Equal("injected failure", Assert.Throws<IOException>(worker.Stop).Message);
    }

    [Fact]
    public void DisposingTwiceStopsTheWorkerOnceAndDiscardsItsFailure()
    {
        var engine = new LinksEngine<uint>(new Log { Fail = true });
        Commit(engine);
        var worker = new BackgroundCompaction<uint>(engine, 1, Period);
        Until(() => Refuses(engine));
        worker.Dispose();
        worker.Dispose();
    }
}
