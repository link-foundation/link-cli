using Foundation.Data.Doublets.Cli.Engine;
using static Foundation.Data.Doublets.Cli.Tests.EngineHistoryTests;
using Random = Foundation.Data.Doublets.Cli.Tests.EngineHistoryTests.Random;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>Deterministic simulation of an etcd-like Raft group with its own
/// timers, skewed and jumping clocks, and an unreliable operating system and
/// network. Mirrors rust/tests/engine_raft_simulation_tests.rs: the same
/// generator, seeds and checks, and the same counts
/// (experiments/issue-138/raft-simulation-expected.txt).
///
/// <see cref="EngineSimulationTests"/> checks <see cref="ClusterEngine{T}"/>
/// members against one idealised atomic store. Here the store itself is three
/// replicas of the draft <see cref="RaftNode"/>, the algorithm etcd runs, and
/// the simulation owns everything etcd would get from the operating system:
/// <list type="bullet">
/// <item><b>Clocks and timers.</b> Each replica reads its own clock, which runs
/// at its own rate (0.5x to 2x real time), jumps forward and backward, and
/// changes rate during the run. Election timeouts, heartbeats, the CheckQuorum
/// interval and the client deadlines are all measured on these skewed
/// clocks.</item>
/// <item><b>Processes.</b> Replicas crash and restart, keeping only their hard
/// state and log, as a process that fsyncs them would.</item>
/// <item><b>Network.</b> Messages are dropped, delayed and reordered. A replica
/// can be isolated, <em>deaf</em> (it sends but does not receive) or
/// <em>mute</em> (it receives but does not send).</item>
/// </list>
///
/// Clients on every member run random transactions through
/// <see cref="ClusterEngine{T}"/>, whose store calls propose a command to their
/// own replica and wait, on that replica's skewed clock, for the replica to
/// apply it. The Elle-style checker of <see cref="EngineHistoryTests"/>
/// searches every history for anomalies, real-time edges included, and every
/// replica must end with the same committed log and state.
///
/// <b>Liveness while faults last.</b> With at most one replica faulty at a time
/// — the failure a three-member group is built to survive — faults never stop,
/// yet the gap between successive acknowledged commits must stay below a bound.
/// A negative control turns PreVote and CheckQuorum off and shows that the same
/// fault schedules then stall the group far longer: a deaf or mute replica keeps
/// disrupting it, which is why etcd has both options.
///
/// <b>Safety under any faults.</b> With every replica allowed to fail at once,
/// progress may stop, but the history must stay strictly serializable.</summary>
public class EngineRaftSimulationTests(Xunit.Abstractions.ITestOutputHelper output)
{
    private static bool PerMille(Random random, ulong chance) => random.Below(1000) < chance;
    private static IOException Failure(string what) => new("injected " + what);

    // -----------------------------------------------------------------------
    // Simulated clocks, processes and network
    // -----------------------------------------------------------------------

    private const int Replicas = 3;
    /// <summary>Client deadline, in the local clock units of the client's replica.</summary>
    private const ulong Deadline = 400;
    /// <summary>The most global ticks one call may take, whatever the clocks do.</summary>
    private const ulong CallLimit = 20_000;
    /// <summary>Real-time ticks before a probe read is abandoned for a fresh one.</summary>
    private const ulong ProbeTimeout = 300;

    /// <summary>A clock running at <c>Rate</c> per mille of real (global) time since <c>Since</c>.</summary>
    private readonly record struct Clock(ulong Rate, ulong Base, ulong Since)
    {
        public ulong Local(ulong now) => Base + (now - Since) * Rate / 1000;
    }

    private enum Fault
    {
        None,
        /// <summary>Nothing in or out.</summary>
        Isolated,
        /// <summary>Sends, but receives nothing.</summary>
        Deaf,
        /// <summary>Receives, but sends nothing.</summary>
        Mute,
        /// <summary>Crashed; restarts when the fault heals.</summary>
        Down,
    }

    private enum Mode
    {
        /// <summary>At most one replica is faulty at any time.</summary>
        Minority,
        /// <summary>Any number of replicas fail at once.</summary>
        Any,
    }

    private sealed record SimStats
    {
        public ulong Delivered, Dropped, Faults, Restarts, ClockJumps, Leaders, MaxTerm;
        /// <summary>Probe reads answered, and the longest real time between two answers.</summary>
        public ulong Probes, LongestGap;
        /// <summary>The same text as Rust's <c>Debug</c>, so both outputs compare verbatim.</summary>
        public override string ToString() =>
            $"SimStats {{ delivered: {Delivered}, dropped: {Dropped}, faults: {Faults}, restarts: {Restarts}, clock_jumps: {ClockJumps}, " +
            $"leaders: {Leaders}, max_term: {MaxTerm}, probes: {Probes}, longest_gap: {LongestGap} }}";
    }

    private sealed class Sim
    {
        internal ulong Now;
        /// <summary>Drives the network only.</summary>
        private readonly Random _random;
        /// <summary>Drives faults and clocks only, so a fault schedule does not
        /// depend on the traffic and is the same with and without PreVote and
        /// CheckQuorum.</summary>
        private readonly Random _schedule;
        internal readonly RaftNode[] Nodes = new RaftNode[Replicas];
        private readonly Clock[] _clocks = new Clock[Replicas];
        private readonly Fault[] _faults = new Fault[Replicas];
        private readonly ulong[] _heals = new ulong[Replicas];
        private readonly Mode _mode;
        internal bool Chaos;
        /// <summary>Messages in flight, by delivery time and send order. The keys
        /// are unique, so the queue yields them in the order of Rust's BTreeMap.</summary>
        private readonly PriorityQueue<RaftEnvelope, (ulong At, ulong Sent)> _queue = new();
        private ulong _sent, _nextId, _leaderTerm;
        internal readonly SimStats Stats = new();
        /// <summary>The transaction a member is committing, and every exchange's caller.</summary>
        internal int Caller;
        internal readonly Dictionary<ulong, int> Callers = new();
        /// <summary>The probe read in flight: replica, command id and when it started.</summary>
        private (int Node, ulong Id, ulong Since)? _probe;
        internal ulong LastAnswer;

        internal Sim(ulong seed, Mode mode, bool preVoteAndCheckQuorum)
        {
            _schedule = new Random(unchecked(seed * 6151));
            for (var id = 0; id < Replicas; id++)
            {
                var rate = 500 + _schedule.Below(1501);
                var start = _schedule.Below(10_000);
                _clocks[id] = new Clock(rate, start, 0);
                var config = new RaftConfig(id, Replicas)
                {
                    Seed = unchecked(seed * 31 + (ulong)id),
                    PreVote = preVoteAndCheckQuorum,
                    CheckQuorum = preVoteAndCheckQuorum,
                };
                Nodes[id] = new RaftNode(config, start);
            }
            _random = new Random(unchecked(seed * 7919));
            _mode = mode;
        }

        internal ulong Local(int node) => _clocks[node].Local(Now);

        private bool CanSend(int node) => _faults[node] is Fault.None or Fault.Deaf;
        private bool CanReceive(int node) => _faults[node] is Fault.None or Fault.Mute;

        private void Flush()
        {
            for (var node = 0; node < Replicas; node++)
                foreach (var envelope in Nodes[node].TakeMessages())
                {
                    if (!CanSend(node) || (Chaos && PerMille(_random, 30)))
                    {
                        Stats.Dropped++;
                        continue;
                    }
                    var delay = 1 + _random.Below(10);
                    if (Chaos && PerMille(_random, 50)) delay += _random.Below(150);
                    _sent++;
                    _queue.Enqueue(envelope, (Now + delay, _sent));
                }
        }

        internal void Heal(int node)
        {
            if (_faults[node] == Fault.Down)
            {
                Nodes[node].Restart(Local(node));
                Stats.Restarts++;
            }
            _faults[node] = Fault.None;
        }

        private void Inject()
        {
            for (var node = 0; node < Replicas; node++)
                if (_faults[node] != Fault.None && Now >= _heals[node]) Heal(node);
            if (!PerMille(_schedule, 3)) return;
            var target = (int)_schedule.Below(Replicas);
            var kind = _schedule.Below(7);
            switch (kind)
            {
                case <= 3:
                    if (_mode == Mode.Minority)
                        for (var other = 0; other < Replicas; other++)
                            if (other != target && _faults[other] != Fault.None) Heal(other);
                    if (_faults[target] != Fault.None) Heal(target);
                    _faults[target] = new[] { Fault.Isolated, Fault.Deaf, Fault.Mute, Fault.Down }[kind];
                    _heals[target] = Now + 200 + _schedule.Below(1800);
                    Stats.Faults++;
                    break;
                case 4 or 5:
                    {
                        // NTP steps the clock forward or backward.
                        var local = Local(target);
                        var step = _schedule.Below(400);
                        var jumped = kind == 4 ? local + step : local - Math.Min(local, step);
                        _clocks[target] = new Clock(_clocks[target].Rate, jumped, Now);
                        Stats.ClockJumps++;
                        break;
                    }
                default:
                    {
                        // The clock drifts at a new rate.
                        var local = Local(target);
                        var rate = 500 + _schedule.Below(1501);
                        _clocks[target] = new Clock(rate, local, Now);
                        break;
                    }
            }
        }

        internal void Step()
        {
            Now++;
            if (Chaos) Inject();
            while (_queue.TryPeek(out var envelope, out var key) && key.At <= Now)
            {
                _queue.Dequeue();
                if (!CanReceive(envelope.To))
                {
                    Stats.Dropped++;
                    continue;
                }
                Stats.Delivered++;
                Nodes[envelope.To].Step(Local(envelope.To), envelope);
            }
            for (var node = 0; node < Replicas; node++)
                if (_faults[node] != Fault.Down) Nodes[node].Tick(Local(node));
            for (var node = 0; node < Replicas; node++)
            {
                Stats.MaxTerm = Math.Max(Stats.MaxTerm, Nodes[node].Term);
                if (Nodes[node].Role == RaftRole.Leader && Nodes[node].Term > _leaderTerm)
                {
                    _leaderTerm = Nodes[node].Term;
                    Stats.Leaders++;
                }
            }
            if (Chaos) Probe();
            Flush();
        }

        /// <summary>Measures availability apart from the clients, whose calls run
        /// one at a time: a probe keeps a linearizable read in flight on a healthy
        /// replica, and the longest real time between two answers is the longest
        /// time the group could not serve a healthy replica.</summary>
        private void Probe()
        {
            if (_probe is var (inFlight, probeId, since))
            {
                if (Nodes[inFlight].TakeOutcome(probeId) is not null)
                {
                    Stats.Probes++;
                    Stats.LongestGap = Math.Max(Stats.LongestGap, Now - LastAnswer);
                    LastAnswer = Now;
                    _probe = null;
                }
                else if (Now - since > ProbeTimeout || _faults[inFlight] != Fault.None)
                {
                    Nodes[inFlight].Forget(probeId);
                    _probe = null;
                }
            }
            if (_probe is not null) return;
            for (var offset = 0; offset < Replicas; offset++)
            {
                var node = (int)((Now + (ulong)offset) % Replicas);
                if (_faults[node] != Fault.None) continue;
                var id = ++_nextId;
                if (Nodes[node].Propose(Local(node), new RaftCommand.Read(id, Array.Empty<string>()))) _probe = (node, id, Now);
                return;
            }
        }

        /// <summary>Proposes on <paramref name="member"/>'s replica and waits, on
        /// that replica's clock, for it to apply the command.</summary>
        internal RaftOutcome Call(int member, Func<ulong, RaftCommand> command)
        {
            if (_faults[member] == Fault.Down) throw Failure("refused connection");
            var id = ++_nextId;
            var proposal = command(id);
            if (proposal is RaftCommand.CompareExchange) Callers[id] = Caller;
            var (start, started) = (Local(member), Now);
            var proposed = false;
            while (true)
            {
                if (_faults[member] == Fault.Down) throw Failure("connection reset");
                if (!proposed)
                {
                    // Without a known leader nothing was sent; proposing again
                    // cannot duplicate the command.
                    proposed = Nodes[member].Propose(Local(member), proposal);
                    Flush();
                }
                if (Nodes[member].TakeOutcome(id) is { } outcome) return outcome;
                if (Local(member) >= start + Deadline)
                {
                    Nodes[member].Forget(id);
                    throw Failure("deadline exceeded");
                }
                Assert.True(Now - started < CallLimit, "a call never ended");
                Step();
            }
        }
    }

    private sealed class Member(Sim sim, int id) : IAtomicJournalStore
    {
        public bool SupportsDurableFlush => false;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys)
        {
            var copy = keys.ToArray();
            return sim.Call(id, command => new RaftCommand.Read(command, copy)) is RaftOutcome.Read read
                ? read.Values
                : throw new InvalidOperationException("a read returned an exchange");
        }
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            var exchange = (Keys: keys.ToArray(), Expected: expected.ToArray(), Updates: updates.Select(update => (IReadOnlyList<string>?)update?.ToArray()).ToArray());
            return sim.Call(id, command => new RaftCommand.CompareExchange(command, exchange.Keys, exchange.Expected, exchange.Updates)) is RaftOutcome.Exchanged exchanged
                ? exchanged.Applied
                : throw new InvalidOperationException("an exchange returned a read");
        }
    }

    // -----------------------------------------------------------------------
    // Client histories
    // -----------------------------------------------------------------------

    private static readonly string[] Shards = ["a", "b"];
    private const uint ShardLinks = 2;
    private const ulong Clients = 5;
    private const int Transactions = 80;
    private const uint FirstWrite = 1000;

    private static uint Key(int shard, uint link) => 10 * ((uint)shard + 1) + link;

    private enum Kind { Read, Write, Scan, Commit, Rollback }
    private readonly record struct Op(Kind Kind, int Shard = 0, uint Link = 0);

    private sealed class Client
    {
        public required int Member, Id;
        public required int[] Shards;
        public ClusterTransaction<uint>? Session;
        public required List<Op> Ops;
    }

    private sealed record Stats
    {
        public ulong Committed, Conflicts, UnknownCommitted, UnknownAborted, Unavailable;
        public SimStats Sim = new();
        public override string ToString() =>
            $"Stats {{ committed: {Committed}, conflicts: {Conflicts}, unknown_committed: {UnknownCommitted}, " +
            $"unknown_aborted: {UnknownAborted}, unavailable: {Unavailable}, sim: {Sim} }}";
    }

    private static void Observe(Transaction record, uint link, uint target)
    {
        var own = record.Writes.FindLastIndex(write => write.Link == link);
        if (own >= 0) record.OwnReads.Add((link, target, record.Writes[own].Value));
        else record.Reads.Add((link, target));
    }

    private static (History History, Stats Stats) RaftHistory(ulong seed, Mode mode, bool preVoteAndCheckQuorum)
    {
        var sim = new Sim(seed, mode, preVoteAndCheckQuorum);
        // One member rewrites journals as checkpoints; the others read them.
        var members = Enumerable.Range(0, Replicas)
            .Select(id => new ClusterEngine<uint>(new Member(sim, id), new EngineOptions(), id == 1 ? 3UL : null)).ToArray();
        // A fault-free start: elect a leader, then install the initial values.
        int Now() => (int)sim.Now;
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
        sim.Caller = 0;
        setup.Commit();
        var records = new List<Transaction> { initial };
        // 0 acknowledged, 1 refused, 2 unknown.
        var outcomes = new List<byte> { 0 };
        sim.Chaos = true;
        sim.LastAnswer = sim.Now;
        var random = new Random(seed);
        var stats = new Stats();
        var clients = new Client?[Clients];
        int started = 0, finished = 0;
        var value = FirstWrite;
        while (finished < Transactions)
        {
            var steps = 1 + random.Below(3);
            for (ulong i = 0; i < steps; i++) sim.Step();
            var index = (int)random.Below(Clients);
            if (clients[index] is null)
            {
                if (started == Transactions) continue;
                started++;
                int[] shards = random.Below(3) switch { 0 => [0], 1 => [1], _ => [0, 1] };
                var ops = new List<Op>();
                var count = 1 + random.Below(4);
                for (ulong i = 0; i < count; i++)
                {
                    var shard = shards[random.Below((ulong)shards.Length)];
                    var link = 1 + (uint)random.Below(ShardLinks);
                    ops.Add(random.Below(10) switch
                    {
                        <= 3 => new Op(Kind.Read, shard, link),
                        <= 7 => new Op(Kind.Write, shard, link),
                        _ => new Op(Kind.Scan, shard),
                    });
                }
                ops.Add(new Op(random.Below(20) == 0 ? Kind.Rollback : Kind.Commit));
                records.Add(new Transaction { Began = Now() });
                outcomes.Add(1);
                clients[index] = new Client { Member = index % Replicas, Id = records.Count - 1, Shards = shards, Ops = ops };
            }
            var client = clients[index]!;
            var record = records[client.Id];
            if (client.Session is null)
            {
                try { client.Session = members[client.Member].Begin(client.Shards.Select(shard => Shards[shard]).ToArray()); }
                catch (IOException)
                {
                    stats.Unavailable++;
                    record.Ended = Now();
                    clients[index] = null;
                    finished++;
                    continue;
                }
            }
            var transaction = client.Session;
            var op = client.Ops[0];
            client.Ops.RemoveAt(0);
            var done = op.Kind is Kind.Commit or Kind.Rollback;
            switch (op.Kind)
            {
                case Kind.Read:
                    Observe(record, Key(op.Shard, op.Link), transaction.Shard(Shards[op.Shard]).GetLink(op.Link)!.Value.Target);
                    break;
                case Kind.Scan:
                    var shardSession = transaction.Shard(Shards[op.Shard]);
                    var found = new List<(uint, uint)>();
                    shardSession.Each(null, link => { found.Add((link![0], link[2])); return shardSession.Constants.Continue; });
                    foreach (var (link, target) in found) Observe(record, Key(op.Shard, link), target);
                    break;
                case Kind.Write:
                    value++;
                    transaction.Shard(Shards[op.Shard]).UpdateLink(op.Link, op.Link, value);
                    record.Writes.Add((Key(op.Shard, op.Link), value));
                    break;
                case Kind.Commit:
                    sim.Caller = client.Id;
                    try { transaction.Commit(); outcomes[client.Id] = 0; }
                    catch (InvalidOperationException error) when (error.Message.StartsWith("Conflict", StringComparison.Ordinal)) { stats.Conflicts++; }
                    catch (IOException error)
                    {
                        Assert.Contains("Indeterminate", error.Message);
                        outcomes[client.Id] = 2;
                    }
                    break;
                case Kind.Rollback:
                    transaction.Rollback();
                    break;
            }
            if (client.Ops.Count == 0) done = true;
            if (done)
            {
                record.Ended = Now();
                clients[index] = null;
                finished++;
            }
        }
        // Heal everything, let the replicas converge, and read the outcome of
        // every exchange from the replicated log.
        sim.Chaos = false;
        for (var node = 0; node < Replicas; node++) sim.Heal(node);
        var healed = sim.Now;
        while (true)
        {
            sim.Step();
            var last = sim.Nodes.Max(node => (ulong)node.Log.Count - 1);
            if (sim.Nodes.Any(node => node.Role == RaftRole.Leader) && sim.Nodes.All(node => node.AppliedIndex == last)) break;
            Assert.True(sim.Now - healed < 5_000, $"seed {seed}: no convergence after heal");
        }
        for (var node = 1; node < Replicas; node++)
        {
            Assert.True(sim.Nodes[node].Log.SequenceEqual(sim.Nodes[0].Log), $"seed {seed}: replicas disagree on the committed log");
            Assert.Equal(sim.Nodes[0].State, sim.Nodes[node].State);
        }
        foreach (var (revision, command) in sim.Nodes[0].State.Exchanges)
        {
            var id = sim.Callers[command];
            var record = records[id];
            Assert.True(record.Revision is null, $"seed {seed}: transaction {id} applied twice");
            record.Revision = revision;
            switch (outcomes[id])
            {
                case 0: stats.Committed++; break;
                case 2:
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
        stats.Sim = sim.Stats with { };
        using var final = members[2].Begin(Shards);
        var finalState = new SortedDictionary<uint, uint>();
        for (var shard = 0; shard < Shards.Length; shard++)
        {
            var session = final.Shard(Shards[shard]);
            session.Each(null, link => { finalState[Key(shard, link![0])] = link[2]; return session.Constants.Continue; });
        }
        return (new History(records, finalState), stats);
    }

    private const ulong Seeds = 20;
    /// <summary>Real-time ticks: the longest a healthy replica may wait for an
    /// answer while one replica at a time keeps failing. Election timeouts are
    /// 100 to 200 local units and clocks run at 0.5x to 2x, so one lost leader
    /// costs up to about 600 ticks; the bound leaves room for a second election
    /// when a vote is lost or a clock steps backward.</summary>
    private const ulong StallBound = 1_500;

    /// <summary>The two lines Rust prints for <paramref name="mode"/>, recorded in
    /// experiments/issue-138/raft-simulation-expected.txt.</summary>
    private static string Expected(Mode mode, bool preVoteAndCheckQuorum) => (mode, preVoteAndCheckQuorum) switch
    {
        (Mode.Minority, true) =>
            "Minority pre-vote/check-quorum=true: Stats { committed: 529, conflicts: 335, unknown_committed: 6, unknown_aborted: 99, unavailable: 605, " +
            "sim: SimStats { delivered: 111972, dropped: 22581, faults: 254, restarts: 60, clock_jumps: 96, leaders: 85, max_term: 8, probes: 5203, longest_gap: 1189 } }\n" +
            "longest stall per seed: [617, 367, 634, 636, 638, 314, 611, 624, 349, 1189, 323, 611, 628, 1103, 624, 610, 931, 640, 469, 554]",
        (Mode.Any, true) =>
            "Any pre-vote/check-quorum=true: Stats { committed: 470, conflicts: 289, unknown_committed: 9, unknown_aborted: 126, unavailable: 682, " +
            "sim: SimStats { delivered: 87923, dropped: 19118, faults: 330, restarts: 74, clock_jumps: 147, leaders: 106, max_term: 10, probes: 3764, longest_gap: 3656 } }\n" +
            "longest stall per seed: [1422, 1332, 2990, 1245, 2539, 314, 1948, 2219, 2574, 3195, 307, 3370, 1980, 1191, 1505, 1005, 1005, 2899, 1141, 3656]",
        _ =>
            "Minority pre-vote/check-quorum=false: Stats { committed: 492, conflicts: 333, unknown_committed: 9, unknown_aborted: 133, unavailable: 607, " +
            "sim: SimStats { delivered: 106356, dropped: 25660, faults: 298, restarts: 69, clock_jumps: 132, leaders: 184, max_term: 64, probes: 4665, longest_gap: 2292 } }\n" +
            "longest stall per seed: [751, 623, 702, 648, 1313, 313, 2292, 606, 1003, 1739, 341, 476, 926, 1046, 675, 741, 1185, 1390, 962, 635]",
    };

    private Stats Run(Mode mode, bool preVoteAndCheckQuorum)
    {
        var sum = new Stats();
        var stalls = new List<ulong>();
        for (ulong seed = 1; seed <= Seeds; seed++)
        {
            var (history, stats) = RaftHistory(seed, mode, preVoteAndCheckQuorum);
            var anomalies = Check(history);
            Assert.True(anomalies.Count == 0, $"seed {seed}: {string.Join("; ", anomalies.Select(a => $"{a.Key} through {string.Join(", ", a.Value)}"))}");
            sum.Committed += stats.Committed;
            sum.Conflicts += stats.Conflicts;
            sum.UnknownCommitted += stats.UnknownCommitted;
            sum.UnknownAborted += stats.UnknownAborted;
            sum.Unavailable += stats.Unavailable;
            sum.Sim.Delivered += stats.Sim.Delivered;
            sum.Sim.Dropped += stats.Sim.Dropped;
            sum.Sim.Faults += stats.Sim.Faults;
            sum.Sim.Restarts += stats.Sim.Restarts;
            sum.Sim.ClockJumps += stats.Sim.ClockJumps;
            sum.Sim.Leaders += stats.Sim.Leaders;
            sum.Sim.MaxTerm = Math.Max(sum.Sim.MaxTerm, stats.Sim.MaxTerm);
            sum.Sim.Probes += stats.Sim.Probes;
            sum.Sim.LongestGap = Math.Max(sum.Sim.LongestGap, stats.Sim.LongestGap);
            stalls.Add(stats.Sim.LongestGap);
        }
        var printed = $"{mode} pre-vote/check-quorum={(preVoteAndCheckQuorum ? "true" : "false")}: {sum}\n" +
            $"longest stall per seed: [{string.Join(", ", stalls)}]";
        output.WriteLine(printed);
        // Determinism parity: the same counts as the Rust reference run.
        Assert.Equal(Expected(mode, preVoteAndCheckQuorum), printed);
        return sum;
    }

    [Fact]
    public void RaftGroupStaysStrictlySerializableAndLiveWhileOneReplicaKeepsFailing()
    {
        var sum = Run(Mode.Minority, true);
        Assert.True(sum.Sim.LongestGap <= StallBound, $"the group stalled for {sum.Sim.LongestGap} ticks: {sum}");
        Assert.True(sum.Committed > 400 && sum.Conflicts > 50 && sum.Sim.Probes > 4000, sum.ToString());
        Assert.True(sum.Sim.Restarts > 0 && sum.Sim.ClockJumps > 0 && sum.Sim.Leaders > Seeds, sum.ToString());
    }

    [Fact]
    public void WithoutPreVoteAndCheckQuorumOneFaultyReplicaStallsTheGroup()
    {
        // The same schedules, without the two options: still safe, but a deaf or
        // mute replica now stops progress for much longer.
        var with = Run(Mode.Minority, true);
        var without = Run(Mode.Minority, false);
        Assert.True(without.Sim.LongestGap > StallBound && without.Sim.MaxTerm > 4 * with.Sim.MaxTerm, $"with: {with}\nwithout: {without}");
    }

    [Fact]
    public void RaftGroupStaysStrictlySerializableWhenEveryReplicaFails()
    {
        var sum = Run(Mode.Any, true);
        Assert.True(sum.Committed > 300, sum.ToString());
        Assert.True(sum.UnknownCommitted > 0 && sum.UnknownAborted > 0, sum.ToString());
        Assert.True(sum.Unavailable > 0, sum.ToString());
    }

    [Fact]
    public void TheSameSeedReplaysTheSameRaftHistory()
    {
        static string Text(History history) => string.Join("\n", history.Transactions.Select(t => $"{t} own [{string.Join(", ", t.OwnReads)}]"));
        var (first, firstStats) = RaftHistory(3, Mode.Any, true);
        var (again, againStats) = RaftHistory(3, Mode.Any, true);
        Assert.Equal(Text(first), Text(again));
        Assert.Equal(firstStats, againStats);
    }
}
