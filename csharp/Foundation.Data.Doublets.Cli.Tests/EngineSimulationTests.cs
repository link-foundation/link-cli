using System.Buffers.Binary;
using Foundation.Data.Doublets.Cli.Engine;
using static Foundation.Data.Doublets.Cli.Tests.EngineHistoryTests;
using Random = Foundation.Data.Doublets.Cli.Tests.EngineHistoryTests.Random;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>Deterministic simulation of an unreliable disk and an unreliable
/// network, as FoundationDB and TigerBeetle test their storage engines: the
/// real code runs against simulated hardware driven by one seeded random
/// source, so every fault schedule replays exactly. Mirrors
/// rust/tests/engine_simulation_tests.rs: the same generator, seeds and checks.
///
/// Disk. <see cref="SimDevice"/> implements the page device over three
/// simulated files (pages and two metadata slots). Each file keeps its durable
/// bytes and the writes issued since its last sync. A simulated power loss
/// keeps, for every 512-byte sector independently, the durable bytes or any
/// prefix of the pending writes to that sector, sometimes half of the next one
/// (a torn sector), and a file length from any prefix of the pending
/// operations: every reordering and loss a volatile disk cache may perform
/// within a flush interval. Writes and syncs also fail at random (the failed
/// call may or may not have reached the cache). The real paged log and engine
/// run several clients on top; after every crash the test reopens the database
/// and requires the exact committed state of one revision between the last
/// acknowledged and the last attempted commit.
///
/// A lying disk acknowledges syncs without persisting anything. No engine can
/// keep its promise over it, so recovery must either refuse the damaged
/// database or return the exact state of an earlier revision, never a mixture.
/// A mirror that includes a lying disk must recover every acknowledged commit;
/// with an honest primary it must also always open, because power loss alone
/// never tears both copies of a metadata decision.
///
/// Network. Several cluster members reach one atomic journal store through
/// simulated links that drop requests, lose replies, deliver requests late
/// (after other transactions ran), and are cut or made one-way by partitions.
/// Clients on every member run random transactions over two shards; the
/// Elle-style checker of <see cref="EngineHistoryTests"/> searches the history
/// for anomalies, including real-time edges between members: the cluster must
/// be strictly serializable. Commits with unknown outcomes are resolved from
/// the store's own log, as a client would read fresh journals after
/// reconnecting.</summary>
public class EngineSimulationTests(Xunit.Abstractions.ITestOutputHelper output)
{
    private static bool PerMille(Random random, ulong chance) => random.Below(1000) < chance;
    private static IOException Injected(string what) => new("injected " + what);

    // -----------------------------------------------------------------------
    // Simulated disk
    // -----------------------------------------------------------------------

    private const int Sector = 512;

    private abstract record Pending;
    private sealed record PendingWrite(int Offset, byte[] Bytes) : Pending;
    private sealed record PendingLength(int Length) : Pending;

    /// <summary>One file: what survives power loss, what the page cache shows,
    /// and the operations between them.</summary>
    private sealed class SimFile
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
    private sealed class Disk(ulong seed, ulong lies, ulong errors)
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
        internal ulong Lied, Dropped, Torn;
        /// <summary>Whether this mutation fails; a failed write may still reach the cache.</summary>
        private bool Fail()
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

    private sealed class SimDevice(Disk disk) : IRepairPageDevice
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

    private const uint Links = 4;
    private const ulong DiskClients = 3;
    private const uint FirstWrite = 1000;

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

    private static string Text(SortedDictionary<uint, uint> state) => "{" + string.Join(", ", state.Select(row => $"{row.Key}: {row.Value}")) + "}";

    /// <summary>Runs <paramref name="cycles"/> rounds of work, each ended by power
    /// loss, and checks every recovery. Throws for any recovery that is not the
    /// exact state of one committed revision.</summary>
    private static CrashStats CrashRun(ulong seed, EngineOptions options, int cycles, Func<IPageDevice> device, Disk[] disks)
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
        _ = new PagedCommitLog(device());
        for (var i = 0; i < disks.Length; i++) disks[i].Errors = errors[i];
        for (var cycle = 0; cycle < cycles; cycle++)
        {
            LinksEngine<uint> db;
            try { db = new LinksEngine<uint>(new PagedCommitLog(device()), options); }
            catch (Exception error) when (error is IOException or InvalidOperationException)
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
            if (random.Below(2) == 0) disks[0].Countdown = random.Below(60);
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

    /// <summary>Different journal encodings, state structures and concurrency
    /// controls write different pages; each must survive the same faults.</summary>
    private static readonly EngineOptions[] DiskConfigurations =
    [
        new(),
        new() { Concurrency = ConcurrencyControl.Pessimistic, Isolation = IsolationLevel.Snapshot, StateStructure = StateStructure.Shared, RevisionEncoding = RevisionEncoding.Changes },
        new() { SerializableValidation = SerializableValidation.ReadSet, StateStructure = StateStructure.SortedRuns, RevisionEncoding = RevisionEncoding.Changes, HistoryLimit = 4 },
        new() { Concurrency = ConcurrencyControl.Exclusive, RequireDurableFlush = true },
    ];

    private const ulong DiskSeeds = 24;
    private const int Cycles = 8;

    private static CrashStats Single(ulong seed, EngineOptions options, ulong lies, ulong errors)
    {
        var shared = new Disk(unchecked(seed * 31), lies, errors);
        return CrashRun(seed, options, Cycles, () => new SimDevice(shared), [shared]);
    }

    private static CrashStats Total(IEnumerable<CrashStats> runs)
    {
        var sum = new CrashStats();
        foreach (var run in runs) sum.Add(run);
        return sum;
    }

    private static IEnumerable<ulong> DiskSeedRange() => Enumerable.Range(1, (int)DiskSeeds).Select(seed => (ulong)seed);

    [Fact]
    public void PowerLossOverReorderingDiskRecoversExactlyAnAcknowledgedOrAttemptedCommit()
    {
        foreach (var options in DiskConfigurations)
        {
            var sum = Total(DiskSeedRange().Select(seed => Single(seed, options, 0, 15)));
            output.WriteLine($"{options.Concurrency} {options.Isolation} {options.StateStructure} {options.RevisionEncoding}: {sum}");
            // An honest disk never loses an acknowledged commit or refuses to open.
            Assert.True((sum.Refused, sum.Lost) == (0, 0), $"{options}: {sum}");
            // The faults happened: unsynced sectors were lost and torn, and failed
            // flushes recovered both ways.
            Assert.True(sum.Commits > 250, $"{options}: {sum}");
            Assert.True(sum.Dropped > 20 && sum.Torn > 4, $"{options}: {sum}");
            Assert.True(sum.UnknownCommitted > 0 && sum.UnknownLost > 0, $"{options}: {sum}");
        }
    }

    [Fact]
    public void LyingDiskIsDetectedOrRollsBackToAnExactEarlierRevisionNeverAMixture()
    {
        var sum = Total(DiskSeedRange().Select(seed => Single(seed, new EngineOptions(), 500, 0)));
        output.WriteLine(sum.ToString());
        // Acknowledged commits are lost, and some recoveries refuse the damage;
        // CrashRun already checked that every opened state is exact.
        Assert.True(sum.Lied > 100, sum.ToString());
        Assert.True(sum.Lost > 0 && sum.Refused > 0, sum.ToString());
    }

    [Fact]
    public void MirrorOfHonestAndLyingDisksKeepsEveryAcknowledgedCommit()
    {
        // (primary lies, secondary lies)
        foreach (var lies in new[] { (false, false), (false, true), (true, false) })
        {
            var sum = Total(DiskSeedRange().Select(seed =>
            {
                var first = new Disk(unchecked(seed * 31), lies.Item1 ? 500UL : 0, 10);
                var second = new Disk(unchecked(seed * 37), lies.Item2 ? 500UL : 0, 10);
                return CrashRun(seed, new EngineOptions(), Cycles,
                    () => new MirroredPageDevice(new SimDevice(first), new SimDevice(second)), [first, second]);
            }));
            output.WriteLine($"{lies}: {sum}");
            Assert.True(sum.Lost == 0, $"{lies} {sum}");
            Assert.True(sum.Commits > 300, $"{lies} {sum}");
            Assert.True(sum.Lied > 100 == (lies != (false, false)), $"{lies} {sum}");
            // The secondary copy of a decision is written only after the primary
            // copy is durable, so power loss alone never tears both copies. Only
            // a primary that lies about its sync can leave both torn; that
            // second fault is refused, never a silent rollback.
            if (!lies.Item1) Assert.True(sum.Refused == 0, $"{lies} {sum}");
        }
    }

    [Fact]
    public void TheSameSeedReplaysTheSameCrashes()
    {
        var options = new EngineOptions();
        Assert.Equal(Single(7, options, 300, 15), Single(7, options, 300, 15));
        Assert.NotEqual(Single(7, options, 300, 15), Single(8, options, 300, 15));
    }

    // -----------------------------------------------------------------------
    // Simulated network
    // -----------------------------------------------------------------------

    private enum LinkState
    {
        Up,
        /// <summary>Nothing gets through.</summary>
        Cut,
        /// <summary>Requests arrive; replies are lost.</summary>
        OneWay,
    }

    private sealed record Request(string[] Keys, JournalVersion[] Expected, IReadOnlyList<string>?[] Updates, int Caller);

    /// <summary>The atomic store and every member's link to it.</summary>
    private sealed class Network(ulong seed)
    {
        internal readonly Random Random = new(seed);
        internal bool Faults;
        internal readonly LinkState[] Links = new LinkState[Members];
        private ulong _revision;
        private readonly Dictionary<string, JournalVersion> _journals = new();
        /// <summary>The transaction id of every applied compare-exchange, by revision.</summary>
        internal readonly SortedDictionary<ulong, int> Applied = new();
        /// <summary>Requests the network holds back and delivers later.</summary>
        internal readonly Queue<Request> Delayed = new();
        /// <summary>The transaction a member is committing; the store logs it.</summary>
        internal int Caller;
        internal ulong DeliveredLate;
        internal JournalVersion Get(string key) => _journals.GetValueOrDefault(key) ?? new(0, Array.Empty<string>());
        internal bool Apply(Request request)
        {
            if (request.Keys.Where((key, i) => Get(key).Generation != request.Expected[i].Generation).Any()) return false;
            var generation = ++_revision;
            for (var i = 0; i < request.Keys.Length; i++)
                if (request.Updates[i] is { } entries) _journals[request.Keys[i]] = new(generation, entries);
            Applied[generation] = request.Caller;
            return true;
        }
        internal void DeliverOneLate()
        {
            if (Delayed.TryDequeue(out var request) && Apply(request)) DeliveredLate++;
        }
        /// <summary>Whether the request reaches the store.</summary>
        internal bool Send(int member) => !Faults || (Links[member] != LinkState.Cut && !PerMille(Random, 40));
        /// <summary>Whether the reply reaches the member.</summary>
        internal bool Reply(int member) => !Faults || (Links[member] == LinkState.Up && !PerMille(Random, 40));
    }

    private sealed class Member(Network network, int id) : IAtomicJournalStore
    {
        public bool SupportsDurableFlush => false;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys)
        {
            if (!network.Send(id)) throw Injected("dropped request");
            var values = keys.Select(network.Get).ToArray();
            if (!network.Reply(id)) throw Injected("lost reply");
            return values;
        }
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            var request = new Request(keys.ToArray(), expected.ToArray(), updates.Select(update => (IReadOnlyList<string>?)update?.ToArray()).ToArray(), network.Caller);
            if (!network.Send(id)) throw Injected("dropped request");
            if (network.Faults && PerMille(network.Random, 60))
            {
                // A slow link: the client times out; the request arrives later.
                network.Delayed.Enqueue(request);
                throw Injected("timeout");
            }
            var applied = network.Apply(request);
            if (!network.Reply(id)) throw Injected("lost reply");
            return applied;
        }
    }

    private static readonly string[] Shards = ["a", "b"];
    private const uint ShardLinks = 2;
    private const int Members = 3;
    private const ulong NetClients = 5;
    private const int NetTransactions = 80;
    private const ulong NetSeeds = 30;

    /// <summary>One key per link of every shard, as the checker sees it.</summary>
    private static uint Key(int shard, uint link) => 10 * ((uint)shard + 1) + link;

    private enum NetKind { Read, Write, Scan, Commit, Rollback }
    private readonly record struct NetOp(NetKind Kind, int Shard = 0, uint Link = 0);

    private sealed class NetClient
    {
        public required int Member, Id;
        public required int[] Shards;
        public ClusterTransaction<uint>? Session;
        public required List<NetOp> Ops;
    }

    public sealed record NetStats
    {
        public ulong Committed, Conflicts, UnknownCommitted, UnknownAborted, DeliveredLate, Unavailable;
    }

    /// <summary>A history of <see cref="NetTransactions"/> transactions by clients
    /// spread over <see cref="Members"/> cluster members.</summary>
    private static (History History, NetStats Stats) NetworkHistory(ulong seed)
    {
        var network = new Network(unchecked(seed * 7919));
        // One member rewrites journals as checkpoints; the others read them.
        var members = Enumerable.Range(0, Members)
            .Select(id => new ClusterEngine<uint>(new Member(network, id), new EngineOptions(), id == 1 ? 3UL : null)).ToArray();
        // Transaction 0 installs the initial values.
        var initial = new Transaction();
        var setup = members[0].Begin(Shards);
        for (var shard = 0; shard < Shards.Length; shard++)
            for (uint link = 1; link <= ShardLinks; link++)
            {
                var session = setup.Shard(Shards[shard]);
                session.EnsureCreated(link);
                session.UpdateLink(link, link, Key(shard, link));
                initial.Writes.Add((Key(shard, link), Key(shard, link)));
            }
        setup.Commit();
        var records = new List<Transaction> { initial };
        // 0 acknowledged, 1 refused, 2 unknown.
        var outcomes = new List<byte> { 0 };
        network.Faults = true;
        var random = new Random(seed);
        var stats = new NetStats();
        var clients = new NetClient?[NetClients];
        int started = 0, finished = 0, now = 0;
        var value = FirstWrite;
        while (finished < NetTransactions)
        {
            now++;
            if (random.Below(25) == 0)
            {
                var member = (int)random.Below(Members);
                network.Links[member] = random.Below(4) switch { 0 => LinkState.Cut, 1 => LinkState.OneWay, _ => LinkState.Up };
            }
            if (random.Below(6) == 0) network.DeliverOneLate();
            var index = (int)random.Below(NetClients);
            if (clients[index] is null)
            {
                if (started == NetTransactions) continue;
                started++;
                int[] shards = random.Below(3) switch { 0 => [0], 1 => [1], _ => [0, 1] };
                var ops = new List<NetOp>();
                var count = 1 + random.Below(4);
                for (ulong i = 0; i < count; i++)
                {
                    var shard = shards[random.Below((ulong)shards.Length)];
                    var link = 1 + (uint)random.Below(ShardLinks);
                    ops.Add(random.Below(10) switch
                    {
                        <= 3 => new NetOp(NetKind.Read, shard, link),
                        <= 7 => new NetOp(NetKind.Write, shard, link),
                        _ => new NetOp(NetKind.Scan, shard),
                    });
                }
                ops.Add(new NetOp(random.Below(20) == 0 ? NetKind.Rollback : NetKind.Commit));
                records.Add(new Transaction { Began = now });
                outcomes.Add(1);
                clients[index] = new NetClient { Member = index % Members, Id = records.Count - 1, Shards = shards, Ops = ops };
            }
            var client = clients[index]!;
            var record = records[client.Id];
            if (client.Session is null)
            {
                try { client.Session = members[client.Member].Begin(client.Shards.Select(shard => Shards[shard]).ToArray()); }
                catch (IOException)
                {
                    // The member could not reach the store: nothing happened.
                    stats.Unavailable++;
                    record.Ended = now;
                    clients[index] = null;
                    finished++;
                    continue;
                }
            }
            var transaction = client.Session;
            var op = client.Ops[0];
            client.Ops.RemoveAt(0);
            var done = op.Kind is NetKind.Commit or NetKind.Rollback;
            switch (op.Kind)
            {
                case NetKind.Read:
                    Observe(record, Key(op.Shard, op.Link), transaction.Shard(Shards[op.Shard]).GetLink(op.Link)!.Value.Target);
                    break;
                case NetKind.Scan:
                    var shardSession = transaction.Shard(Shards[op.Shard]);
                    var found = new List<(uint, uint)>();
                    shardSession.Each(null, link => { found.Add((link![0], link[2])); return shardSession.Constants.Continue; });
                    foreach (var (link, target) in found) Observe(record, Key(op.Shard, link), target);
                    break;
                case NetKind.Write:
                    value++;
                    transaction.Shard(Shards[op.Shard]).UpdateLink(op.Link, op.Link, value);
                    record.Writes.Add((Key(op.Shard, op.Link), value));
                    break;
                case NetKind.Commit:
                    network.Caller = client.Id;
                    try { transaction.Commit(); outcomes[client.Id] = 0; }
                    catch (InvalidOperationException error) when (error.Message.StartsWith("Conflict", StringComparison.Ordinal)) { stats.Conflicts++; }
                    catch (IOException error)
                    {
                        Assert.Contains("Indeterminate", error.Message);
                        outcomes[client.Id] = 2;
                    }
                    break;
                case NetKind.Rollback:
                    transaction.Rollback();
                    break;
            }
            if (client.Ops.Count == 0) done = true;
            if (done)
            {
                record.Ended = now;
                clients[index] = null;
                finished++;
            }
        }
        // Heal the network; requests still in flight arrive.
        while (network.Delayed.Count > 0) network.DeliverOneLate();
        network.Faults = false;
        stats.DeliveredLate = network.DeliveredLate;
        foreach (var (revision, id) in network.Applied)
        {
            var record = records[id];
            record.Revision = revision;
            switch (outcomes[id])
            {
                case 0: stats.Committed++; break;
                case 2:
                    // Unknown to the client: it could have ended any time later.
                    record.Ended = int.MaxValue;
                    stats.UnknownCommitted++;
                    break;
                default: Assert.Fail($"seed {seed}: refused transaction {id} was applied"); break;
            }
        }
        for (var id = 1; id < outcomes.Count; id++)
        {
            if (outcomes[id] == 0 && records[id].Revision is null) Assert.Fail($"seed {seed}: acknowledged transaction {id} is not applied");
            if (outcomes[id] == 2 && records[id].Revision is null) stats.UnknownAborted++;
        }
        // Read the final state through a healed member.
        using var last = members[2].Begin(Shards);
        var finalState = new SortedDictionary<uint, uint>();
        for (var shard = 0; shard < Shards.Length; shard++)
        {
            var session = last.Shard(Shards[shard]);
            session.Each(null, link => { finalState[Key(shard, link![0])] = link[2]; return session.Constants.Continue; });
        }
        return (new History(records, finalState), stats);
    }

    private static void Observe(Transaction record, uint link, uint target)
    {
        var own = record.Writes.FindLastIndex(write => write.Link == link);
        if (own >= 0) record.OwnReads.Add((link, target, record.Writes[own].Value));
        else record.Reads.Add((link, target));
    }

    [Fact]
    public void ClusterMembersOverAFaultyNetworkStayStrictlySerializable()
    {
        var sum = new NetStats();
        var shardsPerCommit = new SortedSet<int>();
        for (ulong seed = 1; seed <= NetSeeds; seed++)
        {
            var (history, stats) = NetworkHistory(seed);
            var anomalies = Check(history);
            Assert.True(anomalies.Count == 0, $"seed {seed}: {string.Join("; ", anomalies.Select(a => $"{a.Key} through {string.Join(", ", a.Value)}"))}");
            foreach (var transaction in history.Transactions.Where(t => t.Revision is not null))
                shardsPerCommit.Add(transaction.Writes.Select(write => write.Link / 10).Distinct().Count());
            sum.Committed += stats.Committed; sum.Conflicts += stats.Conflicts;
            sum.UnknownCommitted += stats.UnknownCommitted; sum.UnknownAborted += stats.UnknownAborted;
            sum.DeliveredLate += stats.DeliveredLate; sum.Unavailable += stats.Unavailable;
        }
        output.WriteLine(sum.ToString());
        // Every kind of outcome happened, including cross-shard commits, late
        // deliveries that still committed, and unknown outcomes both ways.
        Assert.True(shardsPerCommit.Contains(2), string.Join(", ", shardsPerCommit));
        Assert.True(sum.Committed > 500 && sum.Conflicts > 50, sum.ToString());
        Assert.True(sum.UnknownCommitted > 0 && sum.UnknownAborted > 0, sum.ToString());
        Assert.True(sum.DeliveredLate > 0 && sum.Unavailable > 0, sum.ToString());
    }

    [Fact]
    public void TheSameSeedReplaysTheSameNetworkHistory()
    {
        static string Text(History history) => string.Join("\n", history.Transactions.Select(t => $"{t} own [{string.Join(", ", t.OwnReads)}]"));
        var (first, firstStats) = NetworkHistory(3);
        var (again, againStats) = NetworkHistory(3);
        Assert.Equal(Text(first), Text(again));
        Assert.Equal(firstStats, againStats);
    }
}
