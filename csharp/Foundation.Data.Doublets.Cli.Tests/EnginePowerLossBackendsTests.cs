using Foundation.Data.Doublets.Cli.Engine;
using Xunit.Sdk;
using static Foundation.Data.Doublets.Cli.Tests.EngineSimulationDisk;
using Random = Foundation.Data.Doublets.Cli.Tests.EngineHistoryTests.Random;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>Sector-level power loss under the journal-file and LSM backends.
/// Mirrors rust/tests/engine_power_loss_backends_tests.rs.
///
/// <see cref="EngineSimulationTests"/> runs the paged log over a simulated
/// volatile disk: every 512-byte sector independently keeps any prefix of the
/// writes issued since the last sync, sometimes half of the next one, and the
/// file length comes from any prefix of the pending operations. This file puts
/// the two other durable backends on the same disk model.
///
/// Journal file. <see cref="FileCommitLog"/> writes newline-framed records to a
/// real file. Its file stream (<see cref="SimJournalFile"/>) mirrors every
/// write, truncation and sync to a simulated file. A failed append leaves
/// nothing, a prefix or the whole line in the cache; a failed sync persists
/// nothing. At power loss the simulated crash image becomes the real file, and
/// the real <see cref="FileCommitLog"/> (its torn-tail repair) and
/// <see cref="LinksEngine{T}"/> recover it. The repair truncation is a pending
/// length change until the repair's own sync persists it, so a crash or a
/// failed sync can undo it. The shared crash workload then requires the exact
/// state of one revision between the last acknowledged and the last attempted
/// commit.
///
/// LSM. <see cref="PhysicalLsmJournalStore"/> keeps a checkpointed paged
/// write-ahead log (a root and two banks) and one paged file per sorted table.
/// Every one of them is a simulated device; creating and deleting a table is a
/// durable directory operation, as the namespace contract requires. Random
/// groups of keys are compare-exchanged and single keys compare-deleted, with
/// stale guards for conflicts, between flushes and compactions. After every
/// power loss the reopened store must read exactly the key state after one
/// acknowledged or attempted group.
///
/// SQLite is not simulated here: <see cref="SqliteCommitLog"/> requires WAL
/// mode with synchronous=FULL, and SQLite's own crash tests replay power loss
/// through a VFS that loses and tears unsynced sectors.</summary>
public class EnginePowerLossBackendsTests(Xunit.Abstractions.ITestOutputHelper output)
{
    /// <summary>SIM_TRACE=1 prints every step and recovery.</summary>
    private static bool Tracing => Environment.GetEnvironmentVariable("SIM_TRACE") is not null;

    // -----------------------------------------------------------------------
    // Journal file
    // -----------------------------------------------------------------------

    /// <summary>The real journal file, with every write, truncation and sync
    /// mirrored to file 0 of <paramref name="disk"/>.</summary>
    private sealed class SimJournalFile(string path, Disk disk)
        : FileStream(path, FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.Read, bufferSize: 1)
    {
        private SimFile Image => disk.Files[0];
        private void Same()
        {
            if (Length != Image.View.Length) throw new InvalidOperationException($"journal {Length} bytes, simulated {Image.View.Length}");
        }
        public override void Write(byte[] buffer, int offset, int count) => Write(buffer.AsSpan(offset, count));
        public override void Write(ReadOnlySpan<byte> bytes)
        {
            Same();
            var offset = (int)Position;
            if (offset != Image.View.Length) throw new InvalidOperationException("the journal appends at its end");
            var line = bytes.ToArray();
            if (line.Length == 0 || line[^1] != (byte)'\n') throw new InvalidOperationException("the journal writes whole lines");
            if (disk.Fail())
            {
                // The failed call left nothing, a prefix or the whole line.
                var keep = disk.Random.Below(3) switch { 0 => 0, 1 => (int)disk.Random.Below((ulong)line.Length), _ => line.Length };
                Image.Write(offset, line[..keep]);
                base.Write(line, 0, keep);
                throw Injected("append error");
            }
            // FileCommitLog issues the record and its newline in one call; the
            // disk may still persist them apart, as two writes (Rust's writeln!).
            Image.Write(offset, line[..^1]);
            Image.Write(offset + line.Length - 1, line[^1..]);
            // FileStream's span write would call back into the array overload.
            base.Write(line, 0, line.Length);
        }
        /// <summary>The repair truncation: pending until a sync.</summary>
        public override void SetLength(long value)
        {
            Same();
            disk.Mutate(0, file => { file.SetLength((int)value); base.SetLength(value); });
        }
        public override void Flush(bool flushToDisk)
        {
            // The real file is scratch space; only the simulated sync matters.
            if (flushToDisk) disk.Sync(0);
            base.Flush(false);
        }
    }

    /// <summary>Makes the simulated image the real file and recovers it with the
    /// real torn-tail repair of <see cref="FileCommitLog"/>.</summary>
    private sealed class SimJournal : IDisposable
    {
        private readonly string _directory = Directory.CreateTempSubdirectory("power-loss-journal-").FullName;
        private readonly Disk _disk;
        private FileCommitLog? _log;
        internal SimJournal(Disk disk) { _disk = disk; }
        private string Path => System.IO.Path.Combine(_directory, "journal.log");
        internal ICommitLog Open()
        {
            // One writer at a time: the engine of the last round lost power.
            _log?.Dispose();
            _log = null;
            File.WriteAllBytes(Path, _disk.Files[0].View);
            _log = new FileCommitLog(Path, name => new SimJournalFile(name, _disk));
            return _log;
        }
        public void Dispose()
        {
            _log?.Dispose();
            Directory.Delete(_directory, recursive: true);
        }
    }

    /// <summary>The configurations of the Rust port. FileCommitLog has no
    /// per-append sync: the commit's own flush is always the barrier.</summary>
    private static readonly EngineOptions[] JournalConfigurations =
    [
        new(),
        new() { Concurrency = ConcurrencyControl.Pessimistic, Isolation = IsolationLevel.Snapshot, StateStructure = StateStructure.Shared, RevisionEncoding = RevisionEncoding.Changes },
        new() { SerializableValidation = SerializableValidation.ReadSet, StateStructure = StateStructure.SortedRuns, RevisionEncoding = RevisionEncoding.Changes, RequireDurableFlush = true },
    ];

    // A record is a few hundred bytes and is synced at once, so a power cut
    // finds unsynced sectors less often than under the paged log.
    private const ulong JournalSeeds = 48;
    private const int JournalCycles = 8;

    private static CrashStats Journal(ulong seed, EngineOptions options, ulong lies, ulong errors)
    {
        // A record is one write and one sync, not a page set: lose power within
        // fewer mutations, so it lands between them.
        var disk = new Disk(unchecked(seed * 41), lies, errors) { Reach = 8 };
        using var journal = new SimJournal(disk);
        return CrashRun(seed, options, JournalCycles, journal.Open, [disk]);
    }

    private static IEnumerable<ulong> Seeds(ulong count) => Enumerable.Range(1, (int)count).Select(seed => (ulong)seed);

    [Fact]
    public void PowerLossUnderTheJournalFileRecoversExactlyAnAcknowledgedOrAttemptedCommit()
    {
        foreach (var options in JournalConfigurations)
        {
            var sum = Total(Seeds(JournalSeeds).Select(seed => Journal(seed, options, 0, 15)));
            output.WriteLine($"journal file, {options.Concurrency} {options.Isolation} {options.StateStructure} {options.RevisionEncoding}: {sum}");
            // An honest disk never loses an acknowledged commit or refuses to
            // open; the faults happened and failed commits resolved both ways.
            Assert.True((sum.Refused, sum.Lost) == (0, 0), $"{options}: {sum}");
            Assert.True(sum.Commits > 250, $"{options}: {sum}");
            Assert.True(sum.Dropped > 20 && sum.Torn > 4, $"{options}: {sum}");
            Assert.True(sum.UnknownCommitted > 0 && sum.UnknownLost > 0, $"{options}: {sum}");
        }
    }

    [Fact]
    public void ALyingDiskUnderTheJournalFileRollsBackToAnExactRevisionOrIsRefused()
    {
        var sum = Total(Seeds(JournalSeeds).Select(seed => Journal(seed, new EngineOptions(), 500, 0)));
        output.WriteLine($"journal file, lying disk: {sum}");
        // CrashRun checked that every opened state is the exact state of a
        // committed revision; acknowledged commits were lost to the lies.
        Assert.True(sum.Lied > 100 && sum.Lost > 0, sum.ToString());
    }

    [Fact]
    public void TheSameSeedReplaysTheSameJournalFileCrashes()
    {
        var options = new EngineOptions();
        Assert.Equal(Journal(7, options, 300, 15), Journal(7, options, 300, 15));
        Assert.NotEqual(Journal(7, options, 300, 15), Journal(8, options, 300, 15));
    }

    // -----------------------------------------------------------------------
    // LSM
    // -----------------------------------------------------------------------

    /// <summary>Table files. Creating and deleting one is durable at once.</summary>
    private sealed class SimTables(ulong seed, ulong errors) : ILsmTableNamespace
    {
        internal readonly SortedDictionary<ulong, Disk> Files = new();
        internal ulong Created;
        /// <summary>A contract violation; failures of the store must not hide it.</summary>
        internal string? Violation;
        public bool SupportsDurableFlush => true;
        public IReadOnlyList<ulong> List(int maxEntries)
        {
            if (Files.Count > maxEntries) throw new InvalidDataException("too many tables");
            return Files.Keys.ToArray();
        }
        public IPageDevice Create(ulong id)
        {
            if (Files.ContainsKey(id))
            {
                Violation ??= $"table {id} reused";
                throw new InvalidOperationException(Violation);
            }
            Created++;
            var disk = new Disk(unchecked(seed * 1009 + id), 0, errors);
            Files.Add(id, disk);
            return new SimDevice(disk);
        }
        public IPageDevice Open(ulong id) =>
            Files.TryGetValue(id, out var disk) ? new SimDevice(disk) : throw new InvalidDataException($"table {id} is gone");
        public void Close(IPageDevice device) { }
        public void Delete(ulong id) => Files.Remove(id);
    }

    /// <summary>Small limits, so buffers flush and runs compact within a few
    /// groups. A group holds at most BufferKeys keys.</summary>
    private static readonly LsmStoreOptions LsmOptions = new(new LsmOptions(MaxKeys: 32, MaxBytes: 4096, BufferKeys: 2, BufferBytes: 128, MaxRuns: 2, MaxWalRecords: 4, MaxWalBytes: 8192), 16);

    private const ulong KeyCount = 6;
    private const ulong LsmSeeds = 24;
    private const int LsmCycles = 8;

    public sealed record LsmStats
    {
        public ulong Commits, Conflicts, Flushes, Compactions, Tables, Crashes, Refused;
        public ulong UnknownCommitted, UnknownLost, RecoveryErrors, Lied, Dropped, Torn;
        internal void Add(LsmStats run)
        {
            Commits += run.Commits; Conflicts += run.Conflicts; Flushes += run.Flushes; Compactions += run.Compactions;
            Tables += run.Tables; Crashes += run.Crashes; Refused += run.Refused;
            UnknownCommitted += run.UnknownCommitted; UnknownLost += run.UnknownLost; RecoveryErrors += run.RecoveryErrors;
            Lied += run.Lied; Dropped += run.Dropped; Torn += run.Torn;
        }
    }

    /// <summary>Present keys and their entries.</summary>
    private sealed class KeyState : SortedDictionary<string, string[]>
    {
        internal KeyState() : base(StringComparer.Ordinal) { }
        internal KeyState(KeyState other) : base(other, StringComparer.Ordinal) { }
        internal bool Same(KeyState other) =>
            Count == other.Count && this.All(row => other.TryGetValue(row.Key, out var entries) && entries.SequenceEqual(row.Value));
        public override string ToString() => "{" + string.Join(", ", this.Select(row => $"{row.Key}: [{string.Join(", ", row.Value)}]")) + "}";
    }

    private static string Key(ulong index) => "k" + index;

    private static KeyState ReadAll(PhysicalLsmJournalStore store)
    {
        var keys = Enumerable.Range(0, (int)KeyCount).Select(index => Key((ulong)index)).ToArray();
        var versions = store.Read(keys);
        var state = new KeyState();
        for (var i = 0; i < keys.Length; i++)
            if (versions[i].Entries.Count != 0) state[keys[i]] = versions[i].Entries.ToArray();
        return state;
    }

    private static LsmStats LsmRun(ulong seed, ulong lies, ulong errors)
    {
        var wal = Enumerable.Range(0, 3).Select(i => new Disk(unchecked(seed * 43 + (ulong)i), lies, errors)).ToArray();
        var tables = new SimTables(seed, errors);
        Disk[] All() => wal.Concat(tables.Files.Values).ToArray();
        bool Failed() => All().Any(disk => disk.Failed);
        void PowerLoss() { foreach (var disk in All()) disk.PowerLoss(); }
        PhysicalLsmJournalStore Open() => PhysicalLsmJournalStore.Open(
            new CheckpointPagedCommitLog(new SimDevice(wal[0]), new SimDevice(wal[1]), new SimDevice(wal[2]), 128), tables, LsmOptions);
        var random = new Random(seed);
        var stats = new LsmStats();
        // States after each acknowledged or attempted group, oldest first.
        var states = new List<KeyState> { new() };
        int acked = 0, attempted = 0;
        ulong value = 0;
        // Create the store without injected errors, as CrashRun does.
        foreach (var disk in wal) disk.Errors = 0;
        Open().Dispose();
        foreach (var disk in wal) disk.Errors = errors;
        for (var cycle = 0; cycle < LsmCycles; cycle++)
        {
            PhysicalLsmJournalStore? store = null;
            try
            {
                try { store = Open(); }
                catch (Exception error) when (error is not XunitException)
                {
                    if (Failed())
                    {
                        PowerLoss();
                        stats.Crashes++; stats.RecoveryErrors++;
                        continue;
                    }
                    if (Tracing) Console.Error.WriteLine($"seed {seed}: open refused: {error}");
                    stats.Refused++;
                    break;
                }
                var state = ReadAll(store);
                if (Tracing) Console.Error.WriteLine($"seed {seed}: recovered {state}");
                // Only states from the last acknowledged on count: an earlier one
                // would be an acknowledged group lost.
                var at = attempted;
                while (at >= acked && !states[at].Same(state)) at--;
                if (at < acked)
                    throw new InvalidOperationException($"seed {seed}: recovered {state}, expected one of " +
                        string.Join(", ", states.Skip(acked).Take(attempted - acked + 1)));
                if (attempted > acked)
                {
                    if (at == attempted) stats.UnknownCommitted++;
                    else stats.UnknownLost++;
                }
                states.RemoveRange(at + 1, states.Count - at - 1);
                acked = attempted = at;
                if (random.Below(2) == 0)
                {
                    // As Rust evaluates it: the value, then the place.
                    var countdown = random.Below(80);
                    wal[random.Below(3)].Countdown = countdown;
                }
                // A guard read earlier, to make stale compare-exchanges.
                (string[] Keys, IReadOnlyList<JournalVersion> Versions)? stale = null;
                var steps = 5 + random.Below(30);
                for (ulong step = 0; step < steps; step++)
                {
                    if (Failed()) break;
                    var roll = random.Below(100);
                    // The key state if the group commits; none for maintenance.
                    KeyState? next = null;
                    bool? outcome = null;
                    try
                    {
                        if (roll < 8)
                        {
                            stats.Flushes++;
                            store.Flush();
                        }
                        else if (roll < 12)
                        {
                            stats.Compactions++;
                            store.Compact();
                        }
                        else if (roll < 20)
                        {
                            // Delete one key at the generation read now or earlier.
                            var name = Key(random.Below(KeyCount));
                            var old = stale;
                            stale = null;
                            ulong generation;
                            if (old is { } guard && random.Below(4) == 0)
                            {
                                var position = Array.IndexOf(guard.Keys, name);
                                generation = position < 0 ? 1 : guard.Versions[position].Generation;
                            }
                            else generation = store.Read([name])[0].Generation;
                            if (generation == 0) continue;
                            next = new KeyState(states[attempted]);
                            next.Remove(name);
                            outcome = store.CompareDelete(name, generation);
                        }
                        else
                        {
                            var count = 1 + random.Below(2);
                            var chosen = new SortedSet<string>(StringComparer.Ordinal);
                            for (ulong i = 0; i < count; i++) chosen.Add(Key(random.Below(KeyCount)));
                            var keys = chosen.ToArray();
                            var old = stale;
                            stale = null;
                            (string[] Keys, IReadOnlyList<JournalVersion> Versions) guard;
                            if (old is { } earlier && random.Below(4) == 0) guard = earlier;
                            else
                            {
                                var versions = store.Read(keys);
                                if (random.Below(3) == 0) stale = (keys, versions);
                                guard = (keys, versions);
                            }
                            var updates = guard.Keys.Select(_ =>
                            {
                                value++;
                                // Null only guards the key's generation.
                                return random.Below(6) != 0 ? new[] { "v" + value } : null;
                            }).ToArray();
                            next = new KeyState(states[attempted]);
                            for (var i = 0; i < guard.Keys.Length; i++)
                                if (updates[i] is { } entries) next[guard.Keys[i]] = entries;
                            outcome = store.CompareExchange(guard.Keys, guard.Versions, updates);
                        }
                    }
                    catch (Exception error) when (error is not XunitException && Failed())
                    {
                        if (Tracing) Console.Error.WriteLine($"seed {seed}: roll {roll} next {next} -> {error.Message}");
                        // A failed group may still have reached the WAL; a failed
                        // read, flush or compaction keeps the key state.
                        if (next is not null)
                        {
                            states.Add(next);
                            attempted = states.Count - 1;
                        }
                        break;
                    }
                    catch (Exception error) when (error is not XunitException)
                    {
                        throw new InvalidOperationException($"seed {seed}: roll {roll}: {error.Message}", error);
                    }
                    if (Tracing) Console.Error.WriteLine($"seed {seed}: roll {roll} next {next} -> {outcome}");
                    switch (outcome)
                    {
                        case true:
                            states.Add(next!);
                            acked = attempted = states.Count - 1;
                            stats.Commits++;
                            break;
                        case false:
                            stats.Conflicts++;
                            break;
                    }
                }
            }
            finally
            {
                store?.Dispose();
            }
            PowerLoss();
            stats.Crashes++;
        }
        Assert.True(tables.Violation is null, $"seed {seed}: {tables.Violation}");
        stats.Tables = tables.Created;
        foreach (var disk in All()) { stats.Lied += disk.Lied; stats.Dropped += disk.Dropped; stats.Torn += disk.Torn; }
        return stats;
    }

    private static LsmStats Sum(IEnumerable<LsmStats> runs)
    {
        var sum = new LsmStats();
        foreach (var run in runs) sum.Add(run);
        return sum;
    }

    [Fact]
    public void PowerLossUnderTheLsmStoreRecoversExactlyAnAcknowledgedOrAttemptedGroup()
    {
        var total = Sum(Seeds(LsmSeeds).Select(seed => LsmRun(seed, 0, 15)));
        // Deletes are counted with the other acknowledged groups.
        output.WriteLine($"lsm: {total}");
        Assert.True(total.Refused == 0, total.ToString());
        Assert.True(total.Commits > 200 && total.Conflicts > 10, total.ToString());
        Assert.True(total.Tables > 20 && total.Compactions > 10, total.ToString());
        Assert.True(total.Dropped > 20 && total.Torn > 4, total.ToString());
        Assert.True(total.UnknownCommitted > 0 && total.UnknownLost > 0, total.ToString());
    }

    [Fact]
    public void TheSameSeedReplaysTheSameLsmCrashes()
    {
        Assert.Equal(LsmRun(7, 0, 15), LsmRun(7, 0, 15));
        Assert.NotEqual(LsmRun(7, 0, 15), LsmRun(8, 0, 15));
    }
}
