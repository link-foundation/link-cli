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
        public int Reads;
        public bool SupportsDurableFlush => false;
        public IReadOnlyList<string> Read() =>
            ++Reads == FailReadAt ? throw new IOException("read failed") : Inner.Read();
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

    // A later commit that names link 1 and optionally deletes another link.
    private static void Commit(LinksEngine<uint> source, string text, uint? delete = null)
    {
        using var tx = source.Begin();
        tx.SetName(1, text);
        if (delete is { } index) tx.DeleteLink(index);
        tx.Commit();
    }

    [Fact]
    public void AnIncrementalBackupAppendsOnlyChangedRowsAndReopensAsTheSource()
    {
        var source = Engine(null, 2);
        using (var tx = source.Begin())
        {
            for (int i = 0; i < 50; i++) tx.CreateLink(1, 1);
            tx.Commit();
        }
        var copy = new MemoryCommitLog();
        Assert.Equal(3ul, source.Backup(copy));
        var full = copy.Read().ToArray();

        Commit(source, "four");
        Commit(source, "five", 2);
        Assert.Equal(5ul, source.BackupIncremental(copy));
        Assert.Equal(full, copy.Read().Take(full.Length));
        var added = copy.Read().Skip(full.Length).ToArray();
        Assert.Equal(2, added.Length);
        Assert.All(added, record => Assert.StartsWith("engine2|", record));
        Assert.True(added[0].Length * 10 < full[^1].Length);

        // Called again it adds nothing, and later commits continue the same copy.
        Assert.Equal(5ul, source.BackupIncremental(copy));
        Assert.Equal(full.Length + 2, copy.Read().Count);
        Commit(source, "six");
        Assert.Equal(6ul, source.BackupIncremental(copy));

        var restored = new LinksEngine<uint>(copy, new EngineOptions { HistoryLimit = null });
        Assert.Equal(6ul, restored.Snapshot().Revision);
        Assert.Equal(source.Snapshot().Rows.OrderBy(row => row.Key), restored.Snapshot().Rows.OrderBy(row => row.Key));
        Assert.Equal(source.SnapshotAt(4).Rows.OrderBy(row => row.Key), restored.SnapshotAt(4).Rows.OrderBy(row => row.Key));
        using var mine = restored.Begin();
        using var theirs = source.Begin();
        Assert.Equal(theirs.CreateLink(0, 0), mine.CreateLink(0, 0));
    }

    [Fact]
    public void AnEmptyDestinationGetsAFullCopy()
    {
        var source = Engine(null, 3);
        var copy = new MemoryCommitLog();
        Assert.Equal(3ul, source.BackupIncremental(copy));
        Assert.Equal(source.ExportCheckpoint(), copy.Read());
    }

    [Fact]
    public void ACopyOlderThanTheRetainedHistoryIsRefusedAndLeftAlone()
    {
        // A bounded history keeps a recent copy usable and refuses an old one.
        var source = Engine(2, 3);
        var recent = new MemoryCommitLog();
        source.Backup(recent);
        Commit(source, "four");
        Assert.Equal(4ul, source.BackupIncremental(recent));
        var before = recent.Read().ToArray();
        foreach (var text in new[] { "five", "six", "seven" }) Commit(source, text);
        Assert.Throws<KeyNotFoundException>(() => source.BackupIncremental(recent));
        Assert.Equal(before, recent.Read());

        // Without history only a copy of the current revision can continue.
        source = Engine(0, 3);
        var copy = new MemoryCommitLog();
        source.Backup(copy);
        Assert.Equal(3ul, source.BackupIncremental(copy));
        Commit(source, "four");
        Assert.Throws<KeyNotFoundException>(() => source.BackupIncremental(copy));
    }

    [Fact]
    public void ACopyOfAnotherTimelineIsRefusedAndLeftAlone()
    {
        var source = Engine(null, 2);

        // Different rows at a revision both have.
        var other = Engine(null, 1);
        Commit(other, "elsewhere");
        var diverged = new MemoryCommitLog();
        other.Backup(diverged);
        var before = diverged.Read().ToArray();
        Assert.Contains("not a copy", Assert.Throws<InvalidOperationException>(() => source.BackupIncremental(diverged)).Message);
        Assert.Equal(before, diverged.Read());

        // The same rows, but the allocator moved further.
        var plain = new LinksEngine<uint>(new MemoryCommitLog(), new EngineOptions { HistoryLimit = null });
        var moved = new LinksEngine<uint>(new MemoryCommitLog(), new EngineOptions { HistoryLimit = null });
        foreach (var (engine, hole) in new[] { (plain, false), (moved, true) })
        {
            using var tx = engine.Begin();
            tx.CreateLink(0, 0);
            if (hole) { tx.EnsureCreated(7); tx.DeleteLink(7); }
            tx.Commit();
        }
        var allocator = new MemoryCommitLog();
        moved.Backup(allocator);
        Assert.Contains("not a copy", Assert.Throws<InvalidOperationException>(() => plain.BackupIncremental(allocator)).Message);

        // A copy from further along than this engine.
        var ahead = new MemoryCommitLog();
        Engine(null, 5).Backup(ahead);
        Assert.Throws<KeyNotFoundException>(() => source.BackupIncremental(ahead));
        Assert.Throws<KeyNotFoundException>(() => Engine(0, 2).BackupIncremental(ahead));

        // Text that is not a journal.
        var broken = new MemoryCommitLog();
        broken.Append("not a record");
        Assert.ThrowsAny<Exception>(() => source.BackupIncremental(broken));
        Assert.Equal(["not a record"], broken.Read());
    }

    [Fact]
    public void ALiveJournalOfThisEngineContinuesAsACopy()
    {
        // A journal that has no checkpoint record replays from revision zero.
        var log = new MemoryCommitLog();
        var source = new LinksEngine<uint>(log, new EngineOptions { HistoryLimit = null });
        void Write(string text)
        {
            using var tx = source.Begin();
            tx.EnsureCreated(1);
            tx.SetName(1, text);
            tx.Commit();
        }
        Write("one");
        Write("two");
        var copy = new MemoryCommitLog();
        foreach (var record in log.Read()) copy.Append(record);
        Write("three");
        Assert.Equal(3ul, source.BackupIncremental(copy));
        Assert.Equal("three", new LinksEngine<uint>(copy).Snapshot().Rows[1].Name);
    }

    [Fact]
    public void AFailedIncrementCanBeContinuedAndDoesNotFenceTheEngine()
    {
        var source = Engine(null, 2);
        var destination = new Destination();
        source.Backup(destination);
        Commit(source, "three");
        Commit(source, "four");
        var held = destination.Inner.Read().Count;

        destination.FailAppendAt = held + 1;
        Assert.Throws<IOException>(() => source.BackupIncremental(destination));
        Assert.Equal(held + 1, destination.Inner.Read().Count);
        destination.FailAppendAt = null;

        destination.FailFlush = true;
        Assert.Throws<IOException>(() => source.BackupIncremental(destination));
        destination.FailFlush = false;

        // The read before and the read after the copy.
        destination.Reads = 0;
        destination.FailReadAt = 1;
        Assert.Throws<IOException>(() => source.BackupIncremental(destination));
        destination.Reads = 0;
        destination.FailReadAt = 2;
        Assert.Throws<IOException>(() => source.BackupIncremental(destination));
        destination.FailReadAt = null;

        Assert.Equal(4ul, source.BackupIncremental(destination));
        Commit(source, "five");
        Assert.Equal(5ul, source.BackupIncremental(destination));
        Assert.Equal("five", new LinksEngine<uint>(destination.Inner).Snapshot().Rows[1].Name);
    }

    [Fact]
    public void AReadBackThatDiffersFailsAnIncrement()
    {
        var source = Engine(null, 2);
        var lossy = new LossyLog();
        lossy.Append(source.ExportCheckpoint()[0]);
        lossy.Append(source.ExportCheckpoint()[1]);
        lossy.Append(source.ExportCheckpoint()[2]);
        Commit(source, "three");
        var failure = Assert.Throws<InvalidDataException>(() => source.BackupIncremental(lossy));
        Assert.Contains("verification failed", failure.Message);
    }

    [Fact]
    public void CommitsContinueDuringAnIncrementAndStayOutOfIt()
    {
        var source = Engine(null, 2);
        var destination = new Destination();
        source.Backup(destination);
        Commit(source, "three");
        // This would never finish if the commit gate were still held.
        destination.DuringAppend = () =>
            Assert.True(Task.Run(() => Commit(source, "during")).Wait(TimeSpan.FromSeconds(30)), "the copy held the commit gate");
        Assert.Equal(3ul, source.BackupIncremental(destination));
        Assert.Equal(4ul, source.Snapshot().Revision);
        Assert.Equal("three", new LinksEngine<uint>(destination.Inner).Snapshot().Rows[1].Name);
    }

    [Fact]
    public void AnEngineWithAnUnknownJournalOutcomeRefusesAnIncrement()
    {
        var log = new FailingFlushLog();
        var source = new LinksEngine<uint>(log);
        var copy = new MemoryCommitLog();
        source.Backup(copy);
        var before = copy.Read().ToArray();
        using var tx = source.Begin();
        tx.CreateLink(0, 0);
        log.Fail = true;
        Assert.ThrowsAny<Exception>(() => tx.Commit());
        Assert.Contains("reopen", Assert.ThrowsAny<Exception>(() => source.BackupIncremental(copy)).Message, StringComparison.OrdinalIgnoreCase);
        Assert.Equal(before, copy.Read());
    }

    // A chain of two increments on a full copy, with revision 3 as the full copy's end.
    private static (LinksEngine<uint> Source, MemoryCommitLog Archive) Archive()
    {
        var source = Engine(null, 3);
        var archive = new MemoryCommitLog();
        source.BackupIncremental(archive);
        foreach (var text in new[] { "fourth", "fifth" })
        {
            using var tx = source.Begin();
            tx.SetName(1, text);
            tx.Commit();
            source.BackupIncremental(archive);
        }
        return (source, archive);
    }

    private static MemoryCommitLog Restored(ICommitLog archive, ulong? revision)
    {
        var copy = new MemoryCommitLog();
        LinksEngine<uint>.RestoreBackup(archive, copy, revision);
        return copy;
    }

    [Fact]
    public void AnArchiveRestoresEveryRevisionItHoldsAsTheSourceHadIt()
    {
        var (source, archive) = Archive();
        for (ulong revision = 0; revision <= 5; revision++)
        {
            var copy = new MemoryCommitLog();
            Assert.Equal(revision, LinksEngine<uint>.RestoreBackup(archive, copy, revision));
            var restored = new LinksEngine<uint>(copy);
            Assert.Equal(revision, restored.Snapshot().Revision);
            Assert.Equal(source.SnapshotAt(revision).Rows.ToArray(), restored.Snapshot().Rows.ToArray());
        }
        Assert.Equal(archive.Read(), Restored(archive, null).Read());
    }

    [Fact]
    public void ARestoredEngineStartsANewTimelineWithThatRevisionsAllocator()
    {
        var (source, archive) = Archive();
        var early = new LinksEngine<uint>(Restored(archive, 2));
        using (var tx = early.Begin())
        {
            Assert.Equal(2u, tx.CreateLink(0, 0));
            tx.Commit();
        }
        Assert.Equal(3ul, early.Snapshot().Revision);
        Assert.Equal(5ul, source.Snapshot().Revision);
        Assert.Equal("fifth", source.Snapshot().Rows[1].Name);
        // The hole left at revision 3 is part of the allocator that revision restores.
        using var later = new LinksEngine<uint>(Restored(archive, 3)).Begin();
        Assert.Equal(8u, later.CreateLink(0, 0));
    }

    [Fact]
    public void ARestoreCanEndJustBeforeDamageAndRefusesToCrossIt()
    {
        var (_, archive) = Archive();
        var records = archive.Read().ToArray();
        Assert.Equal(6, records.Length);
        var damaged = new MemoryCommitLog();
        for (var index = 0; index < 5; index++) damaged.Append(records[index]);
        var at = records[5].IndexOf('|');
        damaged.Append(records[5].Insert(at + 1, "9"));
        var copy = Restored(damaged, 4);
        Assert.Equal(records.Take(5), copy.Read());
        Assert.Equal(4ul, new LinksEngine<uint>(copy).Snapshot().Revision);
        foreach (var revision in new ulong?[] { 5, null })
        {
            var refused = new MemoryCommitLog();
            Assert.ThrowsAny<Exception>(() => LinksEngine<uint>.RestoreBackup(damaged, refused, revision));
            Assert.Empty(refused.Read());
        }
    }

    [Fact]
    public void ARevisionTheArchiveDoesNotHoldIsRefusedAndWritesNothing()
    {
        var (_, full) = Archive();
        var limited = new MemoryCommitLog();
        Engine(2, 5).Backup(limited);
        foreach (var (archive, revision) in new[] { (full, 6ul), (limited, 3ul), (limited, 0ul) })
        {
            var copy = new MemoryCommitLog();
            var refused = Assert.Throws<KeyNotFoundException>(() => LinksEngine<uint>.RestoreBackup(archive, copy, revision));
            Assert.Contains("not in the backup", refused.Message);
            Assert.Empty(copy.Read());
        }
        // The oldest revision a bounded archive still holds is its checkpoint.
        var first = Assert.Single(Restored(limited, 4).Read());
        Assert.StartsWith("engine3|", first);
    }

    [Fact]
    public void AnEmptyArchiveRestoresOnlyTheEmptyEngine()
    {
        var archive = new MemoryCommitLog();
        Assert.Empty(Restored(archive, null).Read());
        Assert.Empty(Restored(archive, 0).Read());
        Assert.Throws<KeyNotFoundException>(() => Restored(archive, 1));
    }

    [Fact]
    public void AnUntouchedEnginesCheckpointRestoresRevisionZero()
    {
        var archive = new MemoryCommitLog();
        Engine(null, 0).Backup(archive);
        Assert.Single(archive.Read());
        Assert.Single(Restored(archive, 0).Read());
    }

    [Fact]
    public void ANonEmptyRestoreDestinationIsRefusedAndLeftAlone()
    {
        var (_, archive) = Archive();
        var copy = new MemoryCommitLog();
        copy.Append("kept");
        var refused = Assert.Throws<InvalidOperationException>(() => LinksEngine<uint>.RestoreBackup(archive, copy));
        Assert.Contains("must be empty", refused.Message);
        Assert.Equal(["kept"], copy.Read());
    }

    [Fact]
    public void ARestoreRefusesMissingLogs()
    {
        var log = new MemoryCommitLog();
        Assert.Throws<ArgumentNullException>(() => LinksEngine<uint>.RestoreBackup(null!, log));
        Assert.Throws<ArgumentNullException>(() => LinksEngine<uint>.RestoreBackup(log, null!));
    }

    [Fact]
    public void ARestoreReportsAFailedFlushAndADifferingReadBack()
    {
        var (_, archive) = Archive();
        var failing = new FailingFlushLog { Fail = true };
        Assert.Throws<IOException>(() => LinksEngine<uint>.RestoreBackup(archive, failing));
        var verification = Assert.Throws<InvalidDataException>(() => LinksEngine<uint>.RestoreBackup(archive, new LossyLog()));
        Assert.Contains("Restore verification failed", verification.Message);
    }

    [Fact]
    public void ALiveJournalRestoresLikeAnArchive()
    {
        // A journal that has no checkpoint record replays from revision zero.
        var log = new MemoryCommitLog();
        var source = new LinksEngine<uint>(log);
        foreach (var text in new[] { "one", "two", "three" })
        {
            using var tx = source.Begin();
            tx.EnsureCreated(1);
            tx.SetName(1, text);
            tx.Commit();
        }
        var copy = Restored(log, 2);
        Assert.Equal(log.Read().Take(2), copy.Read());
        Assert.Equal("two", new LinksEngine<uint>(copy).Snapshot().Rows[1].Name);
        Assert.Empty(Restored(log, 0).Read());
    }
}
