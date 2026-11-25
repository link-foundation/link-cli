using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>
/// Issue #195: the journal is an append-only file whose size a retention
/// policy bounds, and the older links-backed journal is imported once.
/// </summary>
public sealed class TransactionsJournalCompactionTests : IDisposable
{
    private readonly string _directory = Directory.CreateTempSubdirectory("journal-compaction-").FullName;

    public void Dispose() => Directory.Delete(_directory, recursive: true);

    private string At(string name) => Path.Combine(_directory, name);

    private string Journal => At("journal.log");

    private sealed class Store(string directory, LogRetentionPolicy policy) : IDisposable
    {
        public readonly NamedTypesDecorator<uint> Data = new(Path.Combine(directory, "data.links"));
        public readonly FileTransitionLog Log = FileTransitionLog.Open(Path.Combine(directory, "journal.log"));
        private TransactionsDecorator? _tx;
        public TransactionsDecorator Tx => _tx ??= new TransactionsDecorator(Data, Log, policy);

        public void Dispose()
        {
            _tx?.Dispose();
            Log.Dispose();
            Data.Dispose();
        }
    }

    // The log keeps its append handle open; File.ReadAllText asks for FileShare.Read,
    // which Windows refuses while that write handle is open, so read with its sharing.
    private static string Text(string path)
    {
        using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
        using var reader = new StreamReader(stream);
        return reader.ReadToEnd();
    }

    // Every entry ends with '\n', so the text after the last one is empty.
    private static string[] Lines(string path)
    {
        var lines = Text(path).Split('\n');
        return lines[..^1];
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void RetentionBoundsTheJournalFileAndSurvivesReopening(bool chunked)
    {
        LogRetentionPolicy policy = chunked
            ? new LogRetentionPolicy.Chunked(8, At("archive"))
            : new LogRetentionPolicy.Sized(8);
        long sequence, applied;
        string[] view;
        using (var store = new Store(_directory, policy))
        {
            var largest = 0;
            for (var i = 0; i < 200; i++)
            {
                store.Tx.Create();
                largest = Math.Max(largest, Lines(Journal).Length);
                Assert.Equal(Lines(Journal).Length, store.Tx.JournalEntryCount);
            }
            // Each retained record needs at most three entries; the journal is
            // compacted once it outgrows twice that plus a fixed slack.
            Assert.InRange(largest, 0, 2 * (3 * 8 + 3) + 256 + 3);
            Assert.False(File.Exists(FileTransitionLog.CompactionPath(Journal)));
            sequence = store.Tx.LastLoggedSequence;
            applied = store.Tx.AppliedSequence;
            view = store.Tx.Journal.Select(record => record.Serialize()).ToArray();
        }
        using (var store = new Store(_directory, policy))
        {
            Assert.Equal(sequence, store.Tx.LastLoggedSequence);
            Assert.Equal(applied, store.Tx.AppliedSequence);
            var reopened = store.Tx.Journal.Select(record => record.Serialize()).ToArray();
            Assert.Equal(view, reopened[^view.Length..]);
            store.Tx.Create();
            Assert.Equal(sequence + 1, store.Tx.LastLoggedSequence);
        }
    }

    [Fact]
    public void InfiniteRetentionKeepsEveryEntry()
    {
        using var store = new Store(_directory, LogRetentionPolicy.Default);
        for (var i = 0; i < 150; i++) store.Tx.Create();
        Assert.Equal(150, store.Tx.Journal.Count);
        Assert.Equal(Lines(Journal).Length, store.Tx.JournalEntryCount);
        Assert.True(store.Tx.JournalEntryCount >= 3 * 150);
    }

    [Fact]
    public void ACompactedJournalKeepsRolledBackOwnersAndTheNewestDecision()
    {
        var policy = new LogRetentionPolicy.Sized(2);
        long sequence;
        string decision;
        using (var store = new Store(_directory, policy))
        {
            for (var i = 0; i < 150; i++) store.Tx.Create();
            for (var i = 0; i < 2; i++)
            {
                using var transaction = store.Tx.BeginTransaction();
                store.Tx.Create();
                transaction.Rollback();
            }
            for (var i = 0; i < 60; i++) store.Tx.Create();
            var entries = Lines(Journal);
            Assert.StartsWith("__transactions:sequence:", entries[0]);
            decision = entries.Last(entry => entry.StartsWith("__transactions:commit:", StringComparison.Ordinal));
            sequence = store.Tx.LastLoggedSequence;
        }
        using (var store = new Store(_directory, policy))
        {
            Assert.Equal(sequence, store.Tx.LastLoggedSequence);
            Assert.Contains(decision, Lines(Journal));
        }
    }

    [Fact]
    public void CompactionWaitsForUndecidedOwnersAndSkipsLogsWithoutIt()
    {
        using var store = new Store(_directory, new LogRetentionPolicy.Sized(1));
        for (var i = 0; i < 100; i++) store.Tx.Create();
        // An open transaction keeps its records undecided.
        using (var transaction = store.Tx.BeginTransaction())
        {
            for (var i = 0; i < 300; i++) store.Tx.Create();
            Assert.Equal(Lines(Journal).Length, store.Tx.JournalEntryCount);
            Assert.True(store.Tx.JournalEntryCount > 300);
            transaction.Commit();
        }
        Assert.InRange(store.Tx.JournalEntryCount, 0, 2 * (3 * 1 + 3) + 256);

        // A links-backed journal keeps every entry, as before.
        using var data = new NamedTypesDecorator<uint>(At("other.links"));
        using var links = new NamedTypesDecorator<uint>(At("other.transitions.links"));
        using var tx = new TransactionsDecorator(data, links, new LogRetentionPolicy.Sized(1));
        for (var i = 0; i < 150; i++) tx.Create();
        Assert.True(tx.JournalEntryCount >= 3 * 150);
    }

    [Fact]
    public void TheSequenceWatermarkReservesNumbersAndMustBePositive()
    {
        File.WriteAllText(Journal, "__transactions:sequence:41\n");
        using (var store = new Store(_directory, LogRetentionPolicy.Default))
        {
            Assert.Equal(41, store.Tx.LastLoggedSequence);
            store.Tx.Create();
            Assert.Equal(42, store.Tx.LastLoggedSequence);
        }
        foreach (var bad in new[] { "0", "-3", "x", "" })
        {
            File.WriteAllText(Journal, $"__transactions:sequence:{bad}\n");
            using var store = new Store(_directory, LogRetentionPolicy.Default);
            var error = Assert.Throws<InvalidDataException>(() => store.Tx);
            Assert.Contains("sequence marker", error.Message);
        }
    }

    [Fact]
    public void FileLogCompactionReplacesTheJournalAtomically()
    {
        using var log = FileTransitionLog.Open(Journal);
        Assert.True(log.SupportsCompaction);
        Assert.True(log.SupportsDurableFlush);
        foreach (var entry in new[] { "a", "b", "c" }) log.AppendLogEntry(entry);
        log.CompactLogEntries(["b"]);
        Assert.False(File.Exists(FileTransitionLog.CompactionPath(Journal)));
        log.AppendLogEntry("d");
        Assert.Equal("b\nd\n", Text(Journal));
        Assert.Equal(["b", "d"], log.ReadLogEntries());
        foreach (var bad in new[] { "", "a\nb", "a\rb", "a\0b" })
        {
            Assert.Throws<ArgumentException>(() => log.CompactLogEntries([bad]));
        }
        Assert.Equal(["b", "d"], log.ReadLogEntries());
        log.CompactLogEntries([]);
        Assert.Equal("", Text(Journal));
        log.FlushLog();
    }

    [Fact]
    public void FileLogRefusesLineBreaksAndDiscardsAStaleCompactionCopy()
    {
        File.WriteAllText(Journal, "complete\n");
        File.WriteAllText(FileTransitionLog.CompactionPath(Journal), "partial");
        using var log = FileTransitionLog.Open(Journal);
        Assert.False(File.Exists(FileTransitionLog.CompactionPath(Journal)));
        Assert.Equal(["complete"], log.ReadLogEntries());
        foreach (var bad in new[] { "a\nb", "a\rb", "a\0b" })
        {
            Assert.Throws<ArgumentException>(() => log.AppendLogEntry(bad));
        }
        log.SyncOnAppend = false;
        log.AppendLogEntry("next");
        log.FlushLog();
        Assert.Equal(["complete", "next"], log.ReadLogEntries());
    }

    [Theory]
    [InlineData("", "")]
    [InlineData("a\n\nb\n", "a\n\nb\n")]
    [InlineData("a\nfrag", "a\n")]
    [InlineData("fragment", "")]
    [InlineData("a\nb\0\n", "a\n")]
    [InlineData("a\nb\0\n\nc\0\n", "a\n")]
    [InlineData("\0\n", "")]
    [InlineData("x\0y\nb\n", "x\0y\nb\n")]
    public void OpeningTruncatesTheTornTail(string contents, string kept)
    {
        File.WriteAllText(Journal, contents);
        FileTransitionLog.Open(Journal).Dispose();
        Assert.Equal(kept, Text(Journal));
    }

    [Fact]
    public void TornTailRepairScansAcrossChunks()
    {
        var line = new string('a', 10_000);
        File.WriteAllText(Journal, $"{line}\n{line}\0{line}\n{line}");
        using var log = FileTransitionLog.Open(Journal);
        Assert.Equal([line], log.ReadLogEntries());
    }

    private sealed class MemoryLog : ITransitionLogStore
    {
        public List<string> Entries { get; } = [];
        public void AppendLogEntry(string entry) => Entries.Add(entry);
        public IReadOnlyList<string> ReadLogEntries() => Entries;
        public void FlushLog() { }
    }

    [Fact]
    public void CustomLogsNeitherFlushDurablyNorCompactByDefault()
    {
        ITransitionLogStore log = new MemoryLog();
        Assert.False(log.SupportsDurableFlush);
        Assert.False(log.SupportsCompaction);
        Assert.Throws<NotSupportedException>(() => log.CompactLogEntries([]));
        using var data = new NamedTypesDecorator<uint>(At("memory.links"));
        using var tx = new TransactionsDecorator(data, log, new LogRetentionPolicy.Sized(1));
        Assert.False(tx.SupportsDurableCommit);
        for (var i = 0; i < 150; i++) tx.Create();
        Assert.Equal(((MemoryLog)log).Entries.Count, tx.JournalEntryCount);
        Assert.True(tx.JournalEntryCount >= 3 * 150);
    }

    [Fact]
    public void TheFacadeDisposerReleasesAFileJournal()
    {
        // `using` disposes again afterwards, which every facade tolerates.
        using var data = new NamedTypesDecorator<uint>(At("data.links"));
        using var log = FileTransitionLog.Open(Journal);
        using var tx = new TransactionsDecorator(data, log, new LogRetentionPolicy.Sized(4));
        tx.Create();
        LinksFacadeDisposer.Dispose(tx);
        Assert.Throws<ObjectDisposedException>(() => log.AppendLogEntry("after"));
        foreach (var path in Directory.EnumerateFiles(_directory))
        {
            using var exclusive = new FileStream(path, FileMode.Open, FileAccess.ReadWrite, FileShare.None);
            Assert.True(exclusive.CanWrite);
        }
    }

    /// <summary>A links-backed journal at <paramref name="path"/>, as older versions of <c>clink</c> wrote.</summary>
    private static void LegacyJournal(string path, params string[] entries)
    {
        using var store = new NamedTypesDecorator<uint>(path);
        foreach (var entry in entries)
        {
            var link = store.CreateAndUpdate(store.Constants.Null, store.Constants.Null);
            store.SetName(link, entry);
        }
        store.Flush();
    }

    [Fact]
    public void TheCliJournalLivesBesideTheDatabase()
    {
        Assert.Equal(Path.Combine("d", "db.transitions.log"), TransitionsJournal.MakeTransitionsLogFilename(Path.Combine("d", "db.links")));
        Assert.Equal("db.transitions.links", TransitionsJournal.MakeLegacyTransitionsFilename("db.links"));
        Assert.Equal("db.transitions.links", TransactionsDecorator.MakeTransitionsDatabaseFilename("db.links"));
    }

    [Fact]
    public void IsLinksJournalTellsALinksStoreFromATextJournal()
    {
        string Write(string name, byte[] contents)
        {
            File.WriteAllBytes(At(name), contents);
            return At(name);
        }
        Assert.False(TransitionsJournal.IsLinksJournal(At("missing")));
        Assert.False(TransitionsJournal.IsLinksJournal(Write("empty", [])));
        Assert.False(TransitionsJournal.IsLinksJournal(Write("journal", "__transactions:commit:1\n"u8.ToArray())));
        Assert.False(TransitionsJournal.IsLinksJournal(Write("short", "__trans"u8.ToArray())));
        Assert.True(TransitionsJournal.IsLinksJournal(Write("binary", [0, 1, 2, 3])));
        LegacyJournal(At("legacy.links"), "__transactions:sequence:1");
        Assert.True(TransitionsJournal.IsLinksJournal(At("legacy.links")));
    }

    [Fact]
    public void OpeningTheJournalImportsTheLegacyStoreOnce()
    {
        var db = At("db.links");
        var legacy = TransitionsJournal.MakeLegacyTransitionsFilename(db);
        string[] entries = ["__transactions:sequence:5", "__transactions:applied:2"];
        LegacyJournal(legacy, entries);
        using (var log = TransitionsJournal.Open(db, null, trace: true))
        {
            Assert.Equal(entries, log.ReadLogEntries());
        }
        Assert.False(File.Exists(legacy));
        Assert.False(File.Exists(At("db.transitions.names.links")));
        Assert.Equal(entries, Lines(TransitionsJournal.MakeTransitionsLogFilename(db)));
        using (var log = TransitionsJournal.Open(db, null))
        {
            Assert.Equal(entries, log.ReadLogEntries());
        }
    }

    [Fact]
    public void AnExplicitLinksBackedJournalIsConvertedInPlace()
    {
        var db = At("db.links");
        var explicitPath = At("custom.links");
        string[] entries = ["__transactions:sequence:3", "__transactions:applied:2"];
        LegacyJournal(explicitPath, entries);
        TransitionsJournal.Open(db, explicitPath).Dispose();
        Assert.Equal(entries, Lines(explicitPath));
        Assert.False(File.Exists(At("custom.names.links")));
        // A text journal at the explicit path is opened as is.
        TransitionsJournal.Open(db, explicitPath).Dispose();
        Assert.Equal(entries, Lines(explicitPath));
    }

    [Fact]
    public void ALegacyStoreLeftByACrashIsRemovedOnlyOnceFullyImported()
    {
        var db = At("db.links");
        var legacy = TransitionsJournal.MakeLegacyTransitionsFilename(db);
        var target = TransitionsJournal.MakeTransitionsLogFilename(db);
        LegacyJournal(legacy, "a", "b");
        File.WriteAllText(target, "a\n");
        Assert.False(TransitionsJournal.ImportLinksJournal(legacy, target, trace: true));
        Assert.True(File.Exists(legacy));
        File.WriteAllText(target, "a\nb\nc\n");
        TransitionsJournal.Open(db, null).Dispose();
        Assert.False(File.Exists(legacy));
        Assert.Equal(["a", "b", "c"], Lines(target));
    }

    [Fact]
    public async Task ClinkKeepsItsJournalInABoundedTextFile()
    {
        var db = At("data.links");
        for (var i = 1; i <= 3; i++)
        {
            Clink.AssertSucceeded(await Clink.RunAsync(
                "--db", db, "--transactions", "--retention", "sized:1", $"() (({i}: {i} {i}))"));
        }
        var journal = At("data.transitions.log");
        Assert.True(File.Exists(journal));
        Assert.False(File.Exists(At("data.transitions.links")));
        Assert.All(Lines(journal), entry => Assert.StartsWith("__transactions:", entry));
    }
}
