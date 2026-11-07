using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>Online backup: a consistent cut copied outside the commit gate.</summary>
public class EngineBackupTests
{
    // Revisions 1..=count: revision N names link 1 "雪 N", revision 3 leaves a hole at 7.
    private static LinksEngine<uint> Engine(int? history, int count)
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog(), new EngineOptions { HistoryLimit = history });
        for (int revision = 1; revision <= count; revision++)
        {
            using var tx = engine.Begin();
            if (revision == 1) tx.CreateLink(0, 0);
            if (revision == 3) { tx.EnsureCreated(7); tx.DeleteLink(7); }
            tx.SetName(1, $"雪 {revision}"); tx.Commit();
        }
        return engine;
    }

    [Theory]
    [InlineData(0)]
    [InlineData(2)]
    [InlineData(null)]
    public void ABackupReopensAtItsRevisionAndContinuesWithTheSameAllocator(int? history)
    {
        var source = Engine(history, 6);
        var copy = new MemoryCommitLog();
        Assert.Equal(6ul, source.Backup(copy));
        var restored = new LinksEngine<uint>(copy, new EngineOptions { HistoryLimit = history });
        Assert.Equal(6ul, restored.Snapshot().Revision);
        Assert.Equal(source.Snapshot().Rows.ToArray(), restored.Snapshot().Rows.ToArray());
        Assert.Equal("雪 6", restored.Snapshot().Rows[1].Name);
        using var tx = restored.Begin();
        Assert.Equal(8u, tx.CreateLink(1, 0));
        tx.Commit();
        Assert.Equal(6ul, source.Snapshot().Revision);
    }

    [Fact]
    public void AnUntouchedEngineBacksUpRevisionZero()
    {
        var copy = new MemoryCommitLog();
        Assert.Equal(0ul, Engine(0, 0).Backup(copy));
        var restored = new LinksEngine<uint>(copy);
        Assert.Equal(0ul, restored.Snapshot().Revision);
        using var tx = restored.Begin();
        Assert.Equal(1u, tx.CreateLink(0, 0));
        tx.Commit();
    }

    [Fact]
    public void ARetainedRevisionRestoresThatPointInTimeAndKeepsOnlyEarlierHistory()
    {
        var source = Engine(null, 6);
        var copy = new MemoryCommitLog();
        Assert.Equal(2ul, source.Backup(copy, 2));
        var restored = new LinksEngine<uint>(copy, new EngineOptions { HistoryLimit = null });
        Assert.Equal(2ul, restored.Snapshot().Revision);
        Assert.Equal("雪 2", restored.Snapshot().Rows[1].Name);
        Assert.NotNull(restored.SnapshotAt(1));
        Assert.Throws<KeyNotFoundException>(() => restored.SnapshotAt(3));
        // The allocator is the target's: revision 3 had not yet retired the hole.
        using var tx = restored.Begin();
        Assert.Equal(2u, tx.CreateLink(0, 0));
        tx.Commit();
        Assert.Equal(source.ExportCheckpoint(), source.ExportCheckpoint(6));
        Assert.Equal(3, source.ExportCheckpoint(2).Count);
    }

    [Theory]
    [InlineData(0, 5ul)]
    [InlineData(2, 3ul)]
    [InlineData(null, 7ul)]
    public void AnUnretainedRevisionIsRefusedAndWritesNothing(int? history, ulong revision)
    {
        var source = Engine(history, 6);
        var copy = new MemoryCommitLog();
        Assert.Throws<KeyNotFoundException>(() => source.Backup(copy, revision));
        Assert.Empty(copy.Read());
        Assert.Throws<KeyNotFoundException>(() => source.ExportCheckpoint(revision));
        Assert.Equal(6ul, source.Backup(copy, 6));
    }

    [Fact]
    public void ANonEmptyOrMissingDestinationIsRefused()
    {
        var source = Engine(0, 1);
        var copy = new MemoryCommitLog();
        copy.Append("existing");
        Assert.Contains("must be empty", Assert.Throws<InvalidOperationException>(() => source.Backup(copy)).Message);
        Assert.Equal(new[] { "existing" }, copy.Read());
        Assert.Throws<ArgumentNullException>(() => source.Backup(null!));
    }

    // Scripted destination around a real log; each hook runs before the operation.
    public sealed class Destination : ICommitLog
    {
        public readonly MemoryCommitLog Inner = new();
        public Action? DuringAppend;
        public int? FailAppendAt, FailReadAt;
        public bool FailFlush;
        public int Flushes;
        private int _reads;
        public bool SupportsDurableFlush => false;
        public IReadOnlyList<string> Read() =>
            ++_reads == FailReadAt ? throw new IOException("read failed") : Inner.Read();
        public void Append(string record)
        {
            DuringAppend?.Invoke();
            if (Inner.Read().Count == FailAppendAt) throw new IOException("append failed");
            Inner.Append(record);
        }
        public void Flush()
        {
            if (FailFlush) throw new IOException("flush failed");
            Flushes++;
        }
    }

    [Fact]
    public void CommitsContinueDuringTheCopyAndStayOutOfIt()
    {
        var source = Engine(null, 2);
        int commits = 0;
        var destination = new Destination
        {
            // From another thread, because this would block if the gate were still held.
            DuringAppend = () =>
            {
                var writer = Task.Run(() =>
                {
                    using var tx = source.Begin();
                    tx.SetName(1, "during");
                    tx.Commit();
                });
                Assert.True(writer.Wait(TimeSpan.FromSeconds(30)), "the copy held the commit gate");
                commits++;
            },
        };
        Assert.Equal(2ul, source.Backup(destination));
        Assert.Equal(3, commits);
        Assert.Equal(5ul, source.Snapshot().Revision);
        var restored = new LinksEngine<uint>(destination.Inner, new EngineOptions { HistoryLimit = null });
        Assert.Equal(2ul, restored.Snapshot().Revision);
        Assert.Equal("雪 2", restored.Snapshot().Rows[1].Name);
    }

    public static TheoryData<string, Func<Destination>> Failures => new()
    {
        { "read failed", () => new Destination { FailReadAt = 1 } },
        { "append failed", () => new Destination { FailAppendAt = 1 } },
        { "flush failed", () => new Destination { FailFlush = true } },
        { "read failed", () => new Destination { FailReadAt = 2 } },
    };

    [Theory]
    [MemberData(nameof(Failures))]
    public void AFailedDestinationNeitherBlocksNorFencesTheEngine(string expected, Func<Destination> destination)
    {
        var source = Engine(null, 2);
        Assert.Contains(expected, Assert.Throws<IOException>(() => source.Backup(destination())).Message);
        using (var tx = source.Begin())
        {
            tx.SetName(1, "after");
            Assert.Equal(3ul, tx.Commit());
        }
        Assert.Equal(3ul, source.Backup(new MemoryCommitLog()));
    }

    // Drops the last record once flushed, so the read-back no longer matches.
    private sealed class LossyLog : ICommitLog
    {
        private readonly List<string> _records = new();
        public bool SupportsDurableFlush => false;
        public IReadOnlyList<string> Read() => _records.ToArray();
        public void Append(string record) => _records.Add(record);
        public void Flush() => _records.RemoveAt(_records.Count - 1);
    }

    [Fact]
    public void AReadBackThatDiffersFailsWithoutFencingTheEngine()
    {
        var source = Engine(null, 2);
        var failure = Assert.Throws<InvalidDataException>(() => source.Backup(new LossyLog()));
        Assert.Contains("verification failed", failure.Message);
        using var tx = source.Begin();
        tx.SetName(1, "after");
        Assert.Equal(3ul, tx.Commit());
    }

    [Fact]
    public void TheCopyIsFlushedAndPrivateWritesStayOutOfIt()
    {
        var source = Engine(0, 1);
        using var pending = source.Begin();
        pending.CreateLink(0, 0);
        var destination = new Destination();
        Assert.Equal(1ul, source.Backup(destination));
        Assert.Equal(1, destination.Flushes);
        pending.Commit();
        Assert.Single(new LinksEngine<uint>(destination.Inner).Snapshot().Rows);
    }

    private sealed class FailingFlushLog : ICommitLog
    {
        private readonly MemoryCommitLog _inner = new();
        public bool Fail;
        public bool SupportsDurableFlush => false;
        public IReadOnlyList<string> Read() => _inner.Read();
        public void Append(string record) => _inner.Append(record);
        public void Flush() { if (Fail) throw new IOException("source flush failed"); }
    }

    [Fact]
    public void AnEngineWithAnUnknownJournalOutcomeRefusesToBackUp()
    {
        var log = new FailingFlushLog();
        var source = new LinksEngine<uint>(log);
        using var tx = source.Begin();
        tx.CreateLink(0, 0);
        log.Fail = true;
        Assert.ThrowsAny<Exception>(() => tx.Commit());
        var copy = new MemoryCommitLog();
        Assert.Contains("reopen", Assert.ThrowsAny<Exception>(() => source.Backup(copy)).Message, StringComparison.OrdinalIgnoreCase);
        Assert.Empty(copy.Read());
        Assert.ThrowsAny<Exception>(() => source.ExportCheckpoint(0));
    }

    [Fact]
    public void ABackupSurvivesReopeningFromADurableFile()
    {
        var directory = Directory.CreateTempSubdirectory("engine-backup-");
        var path = Path.Combine(directory.FullName, "backup.log");
        try
        {
            var source = Engine(2, 4);
            using (var file = new FileCommitLog(path)) Assert.Equal(3ul, source.Backup(file, 3));
            using var reopened = new FileCommitLog(path);
            var restored = new LinksEngine<uint>(reopened);
            Assert.Equal(3ul, restored.Snapshot().Revision);
            Assert.Equal("雪 3", restored.Snapshot().Rows[1].Name);
        }
        finally { directory.Delete(true); }
    }
}
