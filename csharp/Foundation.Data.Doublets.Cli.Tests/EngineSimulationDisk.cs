using System.Buffers.Binary;
using System.Text;
using Foundation.Data.Doublets.Cli.Engine;
using Random = Foundation.Data.Doublets.Cli.Tests.EngineHistoryTests.Random;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>A simulated volatile disk and the crash workload shared by the
/// engine simulations (<see cref="EngineSimulationTests"/> and
/// <see cref="EnginePowerLossBackendsTests"/>): every file keeps its durable
/// bytes and the writes issued since its last sync, and power loss keeps any
/// per-sector prefix of them. Mirrors rust/tests/engine_simulation/disk.rs.</summary>
internal static class EngineSimulationDisk
{
    internal static bool PerMille(Random random, ulong chance) => random.Below(1000) < chance;
    internal static IOException Injected(string what) => new("injected " + what);

    // -----------------------------------------------------------------------
    // Simulated disk
    // -----------------------------------------------------------------------

    internal const int Sector = 512;

    internal abstract record Pending;
    internal sealed record PendingWrite(int Offset, byte[] Bytes) : Pending;
    internal sealed record PendingLength(int Length) : Pending;

    /// <summary>One file: what survives power loss, what the page cache shows,
    /// and the operations between them.</summary>
    internal sealed class SimFile
    {
        internal byte[] Durable = [], View = [];
        internal readonly List<Pending> Pending = new();
        internal void Write(int offset, byte[] bytes)
        {
            if (View.Length < offset + bytes.Length) Array.Resize(ref View, offset + bytes.Length);
            bytes.CopyTo(View, offset);
            Pending.Add(new PendingWrite(offset, bytes));
        }
        internal void SetLength(int length) { Array.Resize(ref View, length); Pending.Add(new PendingLength(length)); }
        internal void Sync() { Durable = View.ToArray(); Pending.Clear(); }
        /// <summary>Applies <paramref name="ops"/> to the bytes of one sector, as they would land on the disk.</summary>
        private static void Replay(byte[] sector, int start, IEnumerable<Pending> ops)
        {
            var end = start + sector.Length;
            foreach (var op in ops)
                switch (op)
                {
                    case PendingWrite(var offset, var bytes):
                        int from = Math.Max(offset, start), to = Math.Min(offset + bytes.Length, end);
                        if (from < to) bytes.AsSpan(from - offset, to - from).CopyTo(sector.AsSpan(from - start));
                        break;
                    case PendingLength(var length) when length < end:
                        sector.AsSpan(Math.Max(length - start, 0)).Clear();
                        break;
                }
        }
        private static bool Touches(Pending op, int start, int end) => op switch
        {
            PendingWrite(var offset, var bytes) => offset < end && offset + bytes.Length > start,
            PendingLength(var length) => length < end,
            _ => false,
        };
        /// <summary>Power loss. Returns how many sectors lost or tore a pending write.</summary>
        internal (ulong Dropped, ulong Torn) Crash(Random random)
        {
            ulong dropped = 0, torn = 0;
            if (Pending.Count > 0)
            {
                // The length after any prefix of the pending operations.
                var cut = (int)random.Below((ulong)Pending.Count + 1);
                var length = Durable.Length;
                foreach (var op in Pending.Take(cut))
                    length = op switch { PendingWrite(var offset, var bytes) => Math.Max(length, offset + bytes.Length), PendingLength(var value) => value, _ => length };
                var result = Durable.ToArray();
                Array.Resize(ref result, length);
                for (var start = 0; start < length; start += Sector)
                {
                    var end = Math.Min(start + Sector, length);
                    var touching = Enumerable.Range(0, Pending.Count).Where(at => Touches(Pending[at], start, end)).ToList();
                    if (touching.Count == 0) continue;
                    // Each sector independently keeps any prefix of its writes.
                    var keep = (int)random.Below((ulong)touching.Count + 1);
                    var sector = result[start..end];
                    if (keep > 0) Replay(sector, start, Pending.Take(touching[keep - 1] + 1));
                    if (keep < touching.Count)
                    {
                        dropped++;
                        if (random.Below(4) == 0)
                        {
                            // The next write lands halfway.
                            var next = sector.ToArray();
                            Replay(next, start, [Pending[touching[keep]]]);
                            next.AsSpan(0, sector.Length / 2).CopyTo(sector);
                            torn++;
                        }
                    }
                    sector.CopyTo(result, start);
                }
                Durable = result;
            }
            View = Durable.ToArray();
            Pending.Clear();
            return (dropped, torn);
        }
    }

    /// <summary>Pages, metadata slot 0 and metadata slot 1, as the file page device keeps them.</summary>
    internal sealed class Disk(ulong seed, ulong lies, ulong errors)
    {
        internal readonly SimFile[] Files = [new(), new(), new()];
        internal readonly Random Random = new(seed);
        /// <summary>Per mille of syncs acknowledged without persisting anything.</summary>
        internal readonly ulong Lies = lies;
        /// <summary>Per mille of writes and syncs that fail.</summary>
        internal ulong Errors = errors;
        /// <summary>After one failure the device fails every mutation until power loss.</summary>
        internal bool Failed;
        /// <summary>Mutations left before the power fails in the middle of the work.</summary>
        internal ulong? Countdown;
        /// <summary>Bound of the mutation count after which <see cref="CrashRun"/> cuts the power.</summary>
        internal ulong Reach = 60;
        internal ulong Lied, Dropped, Torn;
        /// <summary>Whether this mutation fails; a failed write may still reach the cache.</summary>
        internal bool Fail()
        {
            switch (Countdown)
            {
                case 0: Countdown = null; Failed = true; break;
                case { } left: Countdown = left - 1; break;
            }
            if (!Failed && Errors > 0 && PerMille(Random, Errors)) { Failed = true; return true; }
            return Failed;
        }
        internal void Mutate(int file, Action<SimFile> apply)
        {
            if (Fail())
            {
                if (Random.Below(2) == 0) apply(Files[file]);
                throw Injected("write error");
            }
            apply(Files[file]);
        }
        internal void Sync(int file)
        {
            if (Fail()) throw Injected("sync error");
            if (Lies > 0 && PerMille(Random, Lies)) { Lied++; return; }
            Files[file].Sync();
        }
        internal void PowerLoss()
        {
            foreach (var file in Files)
            {
                var (lost, half) = file.Crash(Random);
                Dropped += lost; Torn += half;
            }
            Failed = false; Countdown = null;
        }
    }

    internal sealed class SimDevice(Disk disk) : IRepairPageDevice
    {
        private const int PageSize = PagedCommitLog.PageSize;
        public bool SupportsDurableFlush => true;
        public byte[] NewDatabaseId()
        {
            var id = new byte[16];
            BinaryPrimitives.WriteUInt64LittleEndian(id, disk.Random.Next());
            BinaryPrimitives.WriteUInt64LittleEndian(id.AsSpan(8), disk.Random.Next());
            return id;
        }
        public long DataLength => disk.Files[0].View.Length;
        public byte[]? ReadMetadata(int slot)
        {
            var view = disk.Files[1 + slot].View;
            return view.Length switch { 0 => null, 128 => view.ToArray(), _ => [0] };
        }
        public void WriteMetadata(int slot, ReadOnlySpan<byte> bytes) { var copy = bytes.ToArray(); disk.Mutate(1 + slot, file => file.Write(0, copy)); }
        public void SyncMetadata(int slot) => disk.Sync(1 + slot);
        public byte[] ReadPage(ulong index)
        {
            var view = disk.Files[0].View;
            var start = (long)index * PageSize;
            if (start + PageSize > view.Length) throw Injected("short read");
            return view.AsSpan((int)start, PageSize).ToArray();
        }
        public void WritePage(ulong index, ReadOnlySpan<byte> bytes) { var copy = bytes.ToArray(); disk.Mutate(0, file => file.Write((int)index * PageSize, copy)); }
        public void SyncData() => disk.Sync(0);
        public void TruncateData(long length) => disk.Mutate(0, file => file.SetLength((int)length));
        public void RepairPage(ulong index, ReadOnlySpan<byte> bytes)
        {
            if (disk.Files[0].View.Length < (long)index * PageSize) throw Injected("repair hole");
            WritePage(index, bytes);
        }
    }

    // -----------------------------------------------------------------------
    // Crash workload
    // -----------------------------------------------------------------------

    internal const uint Links = 4;
    internal const ulong DiskClients = 3;
    internal const uint FirstWrite = 1000;

    public sealed record CrashStats
    {
        public ulong Commits, Crashes;
        /// <summary>Opens that refused a damaged database.</summary>
        public ulong Refused;
        /// <summary>Recoveries to a revision before the last acknowledged one.</summary>
        public ulong Lost;
        /// <summary>Commits whose flush failed, then recovered as committed / not committed.</summary>
        public ulong UnknownCommitted, UnknownLost;
        /// <summary>Recoveries interrupted by an injected error, then retried.</summary>
        public ulong RecoveryErrors;
        public ulong Lied, Dropped, Torn;
        internal void Add(CrashStats run)
        {
            Commits += run.Commits; Crashes += run.Crashes; Refused += run.Refused; Lost += run.Lost;
            UnknownCommitted += run.UnknownCommitted; UnknownLost += run.UnknownLost; RecoveryErrors += run.RecoveryErrors;
            Lied += run.Lied; Dropped += run.Dropped; Torn += run.Torn;
        }
    }

    private sealed class DiskClient
    {
        public required EngineSession<uint> Session;
        public required List<(uint Link, bool Write)?> Ops;
        public readonly SortedDictionary<uint, uint> Writes = new();
    }

    internal static string Text(SortedDictionary<uint, uint> state) => "{" + string.Join(", ", state.Select(row => $"{row.Key}: {row.Value}")) + "}";

    /// <summary>Runs <paramref name="cycles"/> rounds of work, each ended by power
    /// loss, and checks every recovery. Throws for any recovery that is not the
    /// exact state of one committed revision.</summary>
    internal static CrashStats CrashRun(ulong seed, EngineOptions options, int cycles, Func<ICommitLog> open, Disk[] disks)
    {
        bool Failed() => disks.Any(disk => disk.Failed);
        var random = new Random(seed);
        var stats = new CrashStats();
        var states = new SortedDictionary<ulong, SortedDictionary<uint, uint>> { [0] = new() };
        ulong acked = 0, attempted = 0;
        var value = FirstWrite;
        // Create the database without injected errors: power loss during the
        // very first metadata write leaves no decision at all, and an unreadable
        // log is refused rather than treated as new.
        var errors = disks.Select(disk => disk.Errors).ToArray();
        foreach (var disk in disks) disk.Errors = 0;
        _ = open();
        for (var i = 0; i < disks.Length; i++) disks[i].Errors = errors[i];
        for (var cycle = 0; cycle < cycles; cycle++)
        {
            LinksEngine<uint> db;
            try { db = new LinksEngine<uint>(open(), options); }
            catch (Exception error) when (error is IOException or InvalidOperationException or InvalidDataException or DecoderFallbackException)
            {
                // Recovery itself truncates and repairs, so it can hit an injected
                // error too; the machine loses power again and retries.
                if (Failed())
                {
                    foreach (var disk in disks) disk.PowerLoss();
                    stats.Crashes++; stats.RecoveryErrors++;
                    continue;
                }
                if (Environment.GetEnvironmentVariable("SIM_TRACE") is not null) Console.Error.WriteLine($"seed {seed}: open refused: {error.Message}");
                stats.Refused++;
                break;
            }
            var snapshot = db.Snapshot();
            var revision = snapshot.Revision;
            var state = new SortedDictionary<uint, uint>(snapshot.Rows.ToDictionary(row => row.Key, row => row.Value.Link.Target));
            if (revision > attempted || !states.TryGetValue(revision, out var expected) || !expected.SequenceEqual(state))
                throw new InvalidOperationException($"seed {seed}: recovered revision {revision} {Text(state)}, expected one of " +
                    $"{{{string.Join(", ", states.Select(s => $"{s.Key}: {Text(s.Value)}"))}}} between acknowledged {acked} and attempted {attempted}");
            if (revision < acked) stats.Lost++;
            if (attempted > acked)
            {
                if (revision == attempted) stats.UnknownCommitted++;
                else stats.UnknownLost++;
            }
            foreach (var later in states.Keys.Where(at => at > revision).ToArray()) states.Remove(later);
            acked = attempted = revision;
            if (revision == 0)
            {
                // Set up every link in one commit, without injected errors.
                errors = disks.Select(disk => disk.Errors).ToArray();
                foreach (var disk in disks) disk.Errors = 0;
                using (var setup = db.Begin())
                {
                    for (uint link = 1; link <= Links; link++) { setup.EnsureCreated(link); setup.UpdateLink(link, link, link); }
                    var at = setup.Commit();
                    states[at] = new(Enumerable.Range(1, (int)Links).ToDictionary(link => (uint)link, link => (uint)link));
                    acked = attempted = at;
                }
                for (var i = 0; i < disks.Length; i++) disks[i].Errors = errors[i];
            }
            // Half of the rounds lose power in the middle of a write or sync,
            // leaving whatever the cache had not yet persisted.
            if (random.Below(2) == 0) disks[0].Countdown = random.Below(disks[0].Reach);
            var clients = new DiskClient?[DiskClients];
            var steps = 5 + random.Below(40);
            for (ulong step = 0; step < steps; step++)
            {
                if (Failed()) break;
                var slot = (int)random.Below(DiskClients);
                if (clients[slot] is null)
                {
                    EngineSession<uint> session;
                    try { session = db.Begin(); }
                    catch (Exception error) when (error is IOException or InvalidOperationException) { continue; }
                    var length = 1 + random.Below(4);
                    var ops = new List<(uint, bool)?>();
                    for (ulong i = 0; i < length; i++)
                    {
                        var link = 1 + (uint)random.Below(Links);
                        ops.Add((link, random.Below(2) == 0));
                    }
                    ops.Add(null);
                    clients[slot] = new DiskClient { Session = session, Ops = ops };
                }
                var client = clients[slot]!;
                var op = client.Ops[0];
                client.Ops.RemoveAt(0);
                bool finished;
                switch (op)
                {
                    case (var link, false):
                        Assert.True(client.Session.GetLink(link) is not null, "links are never deleted");
                        finished = false;
                        break;
                    case (var link, true):
                        value++;
                        try { client.Session.UpdateLink(link, link, value); finished = false; }
                        catch (Exception error) when (error is IOException or InvalidOperationException) { finished = true; }
                        client.Writes[link] = value;
                        break;
                    default:
                        var latest = states.Keys.Last();
                        var next = new SortedDictionary<uint, uint>(states[latest]);
                        foreach (var (link, written) in client.Writes) next[link] = written;
                        try
                        {
                            var at = client.Session.Commit();
                            // A read-only commit reports the revision it read.
                            if (client.Writes.Count != 0 || at != latest)
                            {
                                if (at != latest + 1) throw new InvalidOperationException($"seed {seed}: revision {at} after {latest}");
                                states[at] = next;
                                acked = attempted = at;
                                stats.Commits++;
                            }
                        }
                        catch (Exception error) when ((error is IOException or InvalidOperationException) && Failed())
                        {
                            states[latest + 1] = next;
                            attempted = latest + 1;
                        }
                        catch (Exception error) when (error is IOException or InvalidOperationException) { }
                        finished = true;
                        break;
                }
                if (finished) { client.Session.Dispose(); clients[slot] = null; }
            }
            foreach (var client in clients) client?.Session.Dispose();
            foreach (var disk in disks) disk.PowerLoss();
            stats.Crashes++;
        }
        foreach (var disk in disks) { stats.Lied += disk.Lied; stats.Dropped += disk.Dropped; stats.Torn += disk.Torn; }
        return stats;
    }

    internal static CrashStats Total(IEnumerable<CrashStats> runs)
    {
        var sum = new CrashStats();
        foreach (var run in runs) sum.Add(run);
        return sum;
    }
}
