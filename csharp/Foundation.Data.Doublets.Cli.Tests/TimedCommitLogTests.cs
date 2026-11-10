using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TimedCommitLogTests
{
    /// <summary>Fails on demand, advertises what it is told to, and can be replaced.</summary>
    private sealed class Scripted : IReplaceableCommitLog
    {
        private readonly List<string> _records = new();
        public bool Failing { get; set; }
        public bool SupportsDurableFlush { get; init; }
        public bool SupportsReplacement { get; init; } = true;
        public IReadOnlyList<string> Read() { Check(); return _records.ToArray(); }
        public void Append(string record) { Check(); _records.Add(record); }
        public void Flush() => Check();
        public bool Replace(IReadOnlyList<string> records)
        {
            _records.Clear();
            _records.AddRange(records);
            return true;
        }
        private void Check()
        {
            if (Failing) throw new IOException("injected");
        }
    }

    private static void Commit(LinksEngine<uint> engine, string name)
    {
        using var tx = engine.Begin();
        if (engine.Snapshot().Rows.Count == 0) tx.CreateLink(0, 0);
        tx.SetName(1, name);
        tx.Commit();
    }

    /// <summary>Revisions 1, 2, 3 written at 100, 200, 300.</summary>
    private static (Scripted Journal, Scripted Timeline, LinksEngine<uint> Engine) Written(int? history)
    {
        ulong now = 0;
        var (journal, timeline) = (new Scripted(), new Scripted());
        var engine = new LinksEngine<uint>(
            new TimedCommitLog(journal, timeline, () => now), new EngineOptions { HistoryLimit = history });
        foreach (var (time, name) in new[] { (100ul, "first"), (200ul, "second"), (300ul, "third") })
        {
            now = time;
            Commit(engine, name);
        }
        return (journal, timeline, engine);
    }

    private static string? NameAt(ICommitLog journal, ICommitLog timeline, ulong time)
    {
        var revision = TimedCommitLog.RevisionAt(timeline, time);
        var copy = new MemoryCommitLog();
        Assert.Equal(revision, LinksEngine<uint>.RestoreBackup(journal, copy, revision));
        var restored = new LinksEngine<uint>(copy);
        var snapshot = restored.Snapshot();
        Assert.Equal(revision, snapshot.Revision);
        return snapshot.Rows[1u].Name;
    }

    [Theory]
    [InlineData(100ul, "first")]
    [InlineData(199ul, "first")]
    [InlineData(200ul, "second")]
    [InlineData(299ul, "second")]
    [InlineData(300ul, "third")]
    [InlineData(9999ul, "third")]
    public void ARestoreNamesATimeAndGetsTheStateAtThatTime(ulong time, string name)
    {
        var (journal, timeline, _) = Written(null);
        Assert.Equal(new[] { "1|100", "2|200", "3|300" }, timeline.Read());
        Assert.Equal(name, NameAt(journal, timeline, time));
    }

    [Fact]
    public void ATimeBeforeTheFirstTimestampIsRefused()
    {
        var (_, timeline, _) = Written(null);
        Assert.Contains("No revision was written", Assert.Throws<InvalidOperationException>(
            () => TimedCommitLog.RevisionAt(timeline, 99)).Message);
        Assert.Throws<InvalidOperationException>(() => TimedCommitLog.RevisionAt(new MemoryCommitLog(), 1));
    }

    [Fact]
    public void RecordsAreUnchangedSoTheDecoratorCanBeRemoved()
    {
        var (journal, _, _) = Written(null);
        var plain = new LinksEngine<uint>(journal);
        Assert.Equal(3ul, plain.Snapshot().Revision);
    }

    [Fact]
    public void TimeNeverRunsBackwardsEvenAcrossAReopen()
    {
        ulong now = 500;
        var (journal, timeline) = (new Scripted(), new Scripted());
        var engine = new LinksEngine<uint>(new TimedCommitLog(journal, timeline, () => now));
        Commit(engine, "first");
        now = 400;
        Commit(engine, "second");
        engine = new LinksEngine<uint>(new TimedCommitLog(journal, timeline, () => now));
        Commit(engine, "third");
        Assert.Equal(new[] { "1|500", "2|500", "3|500" }, timeline.Read());
    }

    [Fact]
    public void ARevisionWrittenAgainReplacesWhatTheLostOneClaimed()
    {
        var timeline = new MemoryCommitLog();
        foreach (var entry in new[] { "3|10", "4|20", "5|30", "6|40", "5|70" }) timeline.Append(entry);
        Assert.Equal(4ul, TimedCommitLog.RevisionAt(timeline, 20));
        Assert.Equal(4ul, TimedCommitLog.RevisionAt(timeline, 69));
        Assert.Equal(5ul, TimedCommitLog.RevisionAt(timeline, 70));
    }

    [Theory]
    [InlineData("x|1")]
    [InlineData("1|x")]
    [InlineData("1")]
    public void ADamagedTimelineIsRefusedRatherThanGuessedAt(string entry)
    {
        var timeline = new MemoryCommitLog();
        timeline.Append(entry);
        Assert.Throws<InvalidDataException>(() => TimedCommitLog.RevisionAt(timeline, 1));
        Assert.Throws<InvalidDataException>(() => new TimedCommitLog(new MemoryCommitLog(), timeline));
    }

    [Theory]
    [InlineData("other|1|2")]
    [InlineData("engine1")]
    [InlineData("engine1|x|2")]
    public void ARecordWithoutARevisionReachesNeitherLog(string record)
    {
        var (journal, timeline) = (new MemoryCommitLog(), new MemoryCommitLog());
        var log = new TimedCommitLog(journal, timeline);
        Assert.Throws<InvalidDataException>(() => log.Append(record));
        Assert.Empty(journal.Read());
        Assert.Empty(timeline.Read());
    }

    [Fact]
    public void TheSystemClockIsTheDefault()
    {
        var timeline = new MemoryCommitLog();
        new TimedCommitLog(new MemoryCommitLog(), timeline).Append("engine1|1|2|p|c");
        Assert.True(ulong.Parse(timeline.Read()[0].Split('|')[1]) > 1_600_000_000_000);
    }

    [Fact]
    public void AFailingJournalOrTimelineFailsTheCall()
    {
        var (journal, timeline) = (new Scripted(), new Scripted());
        var log = new TimedCommitLog(journal, timeline, () => 1);

        timeline.Failing = true;
        // The record lands first, so the journal can be ahead of its timeline.
        Assert.Throws<IOException>(() => log.Append("engine1|1|2|p|c"));
        Assert.Single(journal.Read());
        Assert.Throws<IOException>(log.Flush);
        Assert.Throws<IOException>(() => new TimedCommitLog(new MemoryCommitLog(), timeline));
        timeline.Failing = false;

        journal.Failing = true;
        Assert.Throws<IOException>(() => log.Append("engine1|2|2|p|c"));
        Assert.Throws<IOException>(log.Flush);
        Assert.Throws<IOException>(() => log.Read());
        journal.Failing = false;
        Assert.Empty(timeline.Read());
        log.Flush();
        Assert.Single(log.Read());
    }

    [Theory]
    [InlineData(true, true, true)]
    [InlineData(true, false, false)]
    [InlineData(false, true, false)]
    public void DurabilityNeedsBothLogs(bool journal, bool timeline, bool expected)
        => Assert.Equal(expected, new TimedCommitLog(
            new Scripted { SupportsDurableFlush = journal },
            new Scripted { SupportsDurableFlush = timeline }).SupportsDurableFlush);

    [Fact]
    public void CompactionKeepsTheTimelineAndOlderRevisionsAreThenRefused()
    {
        var (journal, timeline, engine) = Written(0);
        Assert.True(engine.CompactJournal());
        Assert.Single(journal.Read());
        Assert.Equal(3, timeline.Read().Count);
        Assert.Equal("third", NameAt(journal, timeline, 300));
        var dropped = TimedCommitLog.RevisionAt(timeline, 100);
        Assert.Contains("not in the backup", Assert.Throws<KeyNotFoundException>(
            () => LinksEngine<uint>.RestoreBackup(journal, new MemoryCommitLog(), dropped)).Message,
            StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void AnInnerLogWithoutReplacementKeepsCompactionUnavailable()
    {
        foreach (ICommitLog inner in new ICommitLog[] { new MemoryCommitLog(), new Scripted { SupportsReplacement = false } })
        {
            var log = new TimedCommitLog(inner, new MemoryCommitLog());
            Assert.False(log.SupportsReplacement);
            Assert.Throws<NotSupportedException>(() => log.Replace(Array.Empty<string>()));
        }
        Assert.True(new TimedCommitLog(new Scripted(), new MemoryCommitLog()).SupportsReplacement);
    }

    [Fact]
    public void ItComposesOverADeltaJournal()
    {
        var timeline = new MemoryCommitLog();
        var engine = new LinksEngine<uint>(
            new TimedCommitLog(new DeltaCommitLog(new MemoryCommitLog(), 8), timeline, () => 5));
        Commit(engine, "first");
        Commit(engine, "second");
        Assert.Equal(new[] { "1|5", "2|5" }, timeline.Read());
        Assert.Equal(2ul, engine.Snapshot().Revision);
    }

    [Fact]
    public void NullArgumentsAreRefused()
    {
        var log = new MemoryCommitLog();
        Assert.Throws<ArgumentNullException>(() => new TimedCommitLog(null!, log));
        Assert.Throws<ArgumentNullException>(() => new TimedCommitLog(log, null!));
        Assert.Throws<ArgumentNullException>(() => TimedCommitLog.RevisionAt(null!, 0));
        Assert.Throws<ArgumentNullException>(() => new TimedCommitLog(log, new MemoryCommitLog()).Append(null!));
    }
}
