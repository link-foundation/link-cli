using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>Long random histories, checked for isolation anomalies by
/// dependency cycles, as Jepsen's Elle does with Adya's model. A seeded
/// scheduler runs four clients against one engine, one step at a time; every
/// write stores a unique value and commit revisions give each link's version
/// order. Mirrors rust/tests/engine_history_tests.rs: the same generator,
/// seeds and checker.</summary>
public class EngineHistoryTests(Xunit.Abstractions.ITestOutputHelper output)
{
    private const int Clients = 4;
    private const uint Keys = 3;
    private const int Transactions = 60;
    private const int Seeds = 20;
    private const uint FirstWrite = 1000;

    /// <summary>splitmix64: the same sequence in Rust, C# and JS.</summary>
    private sealed class Random(ulong state)
    {
        public ulong Next()
        {
            unchecked
            {
                state += 0x9E3779B97F4A7C15UL;
                var z = state;
                z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9UL;
                z = (z ^ (z >> 27)) * 0x94D049BB133111EBUL;
                return z ^ (z >> 31);
            }
        }
        public ulong Below(ulong bound) => Next() % bound;
    }

    private enum Kind { Read, Lock, Scan, Write, Commit, Rollback }
    private readonly record struct Op(Kind Kind, uint Link = 0);

    /// <summary>Up to four operations, then commit or (rarely) rollback.</summary>
    private static List<Op> Plan(Random random, bool pessimistic)
    {
        var length = 1 + random.Below(4);
        var ops = new List<Op>();
        for (ulong i = 0; i < length; i++)
        {
            var kind = random.Below(10);
            var key = 1 + (uint)random.Below(Keys);
            ops.Add(kind switch
            {
                <= 3 => new Op(Kind.Read, key),
                <= 6 or 9 => new Op(Kind.Write, key),
                7 => new Op(Kind.Scan),
                _ => new Op(pessimistic ? Kind.Lock : Kind.Read, key),
            });
        }
        ops.Add(new Op(random.Below(20) == 0 ? Kind.Rollback : Kind.Commit));
        return ops;
    }

    /// <summary>What a transaction did, as a client saw it.</summary>
    public sealed class Transaction
    {
        public List<(uint Link, uint Value)> Reads { get; init; } = new();
        public List<(uint Link, uint Value)> Writes { get; init; } = new();
        /// <summary>Reads after the transaction's own write to that link.</summary>
        public List<(uint Link, uint Read, uint Written)> OwnReads { get; init; } = new();
        public ulong? Revision { get; set; }
        public int Began { get; set; }
        public int Ended { get; set; }

        public override string ToString() =>
            $"reads [{string.Join(", ", Reads)}] writes [{string.Join(", ", Writes)}] revision {Revision} steps {Began}..{Ended}";
    }

    public sealed record History(List<Transaction> Transactions, SortedDictionary<uint, uint> FinalState);

    private sealed class Client
    {
        public EngineSession<uint>? Session;
        public required List<Op> Ops;
        public int Next;
        public required Transaction Record;
    }

    private static void Observe(Transaction record, uint link, uint target)
    {
        var own = record.Writes.FindLastIndex(write => write.Link == link);
        if (own >= 0) record.OwnReads.Add((link, target, record.Writes[own].Value));
        else record.Reads.Add((link, target));
    }

    private static void Step(LinksEngine<uint> engine, Client client, Op op, ref uint value)
    {
        var session = client.Session ??= engine.Begin();
        var record = client.Record;
        switch (op.Kind)
        {
            case Kind.Read: Observe(record, op.Link, session.GetLink(op.Link)!.Value.Target); break;
            case Kind.Lock: Observe(record, op.Link, session.LockForUpdate(op.Link)!.Value.Target); break;
            case Kind.Scan:
                var found = new List<(uint, uint)>();
                session.Each(null, link => { found.Add((link![0], link[2])); return session.Constants.Continue; });
                foreach (var (index, target) in found) Observe(record, index, target);
                break;
            case Kind.Write:
                value++;
                session.UpdateLink(op.Link, op.Link, value);
                record.Writes.Add((op.Link, value));
                break;
            case Kind.Commit: record.Revision = session.Commit(); break;
            case Kind.Rollback: session.Dispose(); client.Session = null; break;
        }
    }

    public static History Generate(EngineOptions options, ulong seed)
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog(), options);
        var initial = new Transaction();
        using (var setup = engine.Begin())
        {
            for (uint link = 1; link <= Keys; link++)
            {
                setup.EnsureCreated(link);
                setup.UpdateLink(link, link, link);
                initial.Writes.Add((link, link));
            }
            initial.Revision = setup.Commit();
        }
        var transactions = new List<Transaction> { initial };
        var random = new Random(seed);
        var pessimistic = options.Concurrency == ConcurrencyControl.Pessimistic;
        var clients = new Client?[Clients];
        int started = 0, finished = 0, now = 0;
        var value = FirstWrite - 1;
        while (finished < Transactions)
        {
            now++;
            var slot = (int)random.Below(Clients);
            if (clients[slot] is null)
            {
                if (started == Transactions) continue;
                started++;
                clients[slot] = new Client { Ops = Plan(random, pessimistic), Record = new Transaction { Began = now } };
            }
            var client = clients[slot]!;
            var op = client.Ops[client.Next++];
            var failed = false;
            try { Step(engine, client, op, ref value); }
            catch (InvalidOperationException)
            {
                // A refused step ends the transaction, as a client would roll back.
                failed = true;
            }
            if (failed || client.Next == client.Ops.Count)
            {
                client.Session?.Dispose();
                if (failed) client.Record.Revision = null;
                client.Record.Ended = now;
                transactions.Add(client.Record);
                clients[slot] = null;
                finished++;
            }
        }
        var final = new SortedDictionary<uint, uint>(engine.Snapshot().Rows.ToDictionary(row => row.Key, row => row.Value.Link.Target));
        return new History(transactions, final);
    }

    public enum Anomaly { Garbage, G1a, G1b, Internal, LostWrite, G1c, GSingle, G2 }

    /// <summary>Every anomaly in the history, each with one example: the
    /// transactions involved.</summary>
    public static SortedDictionary<Anomaly, int[]> Check(History history)
    {
        var transactions = history.Transactions;
        var found = new SortedDictionary<Anomaly, int[]>();
        void Report(Anomaly anomaly, params int[] example) => found.TryAdd(anomaly, example);
        var writer = new Dictionary<uint, (int Id, bool Last)>();
        for (var id = 0; id < transactions.Count; id++)
        {
            var writes = transactions[id].Writes;
            for (var at = 0; at < writes.Count; at++)
                writer[writes[at].Value] = (id, !writes.Skip(at + 1).Any(later => later.Link == writes[at].Link));
        }
        var committed = Enumerable.Range(0, transactions.Count).Where(id => transactions[id].Revision is not null).ToList();
        var versions = new SortedDictionary<uint, List<int>>();
        foreach (var id in committed.OrderBy(id => transactions[id].Revision))
            foreach (var link in transactions[id].Writes.Select(write => write.Link).Distinct().Order())
            {
                if (!versions.TryGetValue(link, out var order)) versions[link] = order = new();
                order.Add(id);
            }
        foreach (var (link, writers) in versions)
        {
            var expected = transactions[writers[^1]].Writes.Last(write => write.Link == link).Value;
            if (!history.FinalState.TryGetValue(link, out var actual) || actual != expected) Report(Anomaly.LostWrite, writers.ToArray());
        }
        var n = transactions.Count;
        var dependencies = Enumerable.Range(0, n).Select(_ => new SortedSet<int>()).ToArray();
        var anti = new List<(int From, int To)>();
        foreach (var writers in versions.Values)
            for (var i = 0; i + 1 < writers.Count; i++) dependencies[writers[i]].Add(writers[i + 1]);
        foreach (var id in committed)
        {
            var transaction = transactions[id];
            if (transaction.OwnReads.Any(read => read.Read != read.Written)) Report(Anomaly.Internal, id);
            foreach (var (link, value) in transaction.Reads)
            {
                if (!writer.TryGetValue(value, out var source)) { Report(Anomaly.Garbage, id); continue; }
                if (transactions[source.Id].Revision is null) { Report(Anomaly.G1a, source.Id, id); continue; }
                if (!source.Last) { Report(Anomaly.G1b, source.Id, id); continue; }
                if (source.Id == id) { Report(Anomaly.Internal, id); continue; }
                dependencies[source.Id].Add(id);
                var order = versions[link];
                var position = order.IndexOf(source.Id);
                if (position + 1 < order.Count && order[position + 1] != id) anti.Add((id, order[position + 1]));
            }
        }
        // Real time: a transaction that committed before another began precedes it.
        foreach (var earlier in committed)
            foreach (var later in committed)
                if (transactions[earlier].Ended < transactions[later].Began) dependencies[earlier].Add(later);
        if (CycleIn(dependencies) is { } cycle) Report(Anomaly.G1c, cycle);
        var reach = new Dictionary<int, bool[]>();
        foreach (var (from, to) in anti)
        {
            if (!reach.TryGetValue(to, out var reachable)) reach[to] = reachable = ReachableFrom(dependencies, to);
            if (reachable[from]) Report(Anomaly.GSingle, from, to);
        }
        if (!found.ContainsKey(Anomaly.GSingle) && !found.ContainsKey(Anomaly.G1c))
        {
            var all = dependencies.Select(edges => new SortedSet<int>(edges)).ToArray();
            foreach (var (from, to) in anti) all[from].Add(to);
            if (CycleIn(all) is { } longer) Report(Anomaly.G2, longer);
        }
        return found;
    }

    private static bool[] ReachableFrom(SortedSet<int>[] graph, int start)
    {
        var seen = new bool[graph.Length];
        var stack = new Stack<int>();
        stack.Push(start);
        while (stack.TryPop(out var node))
            foreach (var next in graph[node])
                if (!seen[next]) { seen[next] = true; stack.Push(next); }
        return seen;
    }

    /// <summary>One cycle, as the transactions on it, or null when acyclic.</summary>
    private static int[]? CycleIn(SortedSet<int>[] graph)
    {
        var state = new byte[graph.Length]; // 0 unvisited, 1 on the path, 2 done
        for (var root = 0; root < graph.Length; root++)
        {
            if (state[root] != 0) continue;
            var path = new List<int> { root };
            var iterators = new List<IEnumerator<int>> { graph[root].GetEnumerator() };
            state[root] = 1;
            while (iterators.Count > 0)
            {
                var iterator = iterators[^1];
                if (!iterator.MoveNext())
                {
                    state[path[^1]] = 2;
                    path.RemoveAt(path.Count - 1);
                    iterators.RemoveAt(iterators.Count - 1);
                    continue;
                }
                var next = iterator.Current;
                if (state[next] == 1) return path.Skip(path.IndexOf(next)).ToArray();
                if (state[next] != 0) continue;
                state[next] = 1;
                path.Add(next);
                iterators.Add(graph[next].GetEnumerator());
            }
        }
        return null;
    }

    private static Anomaly[] Allowed(EngineOptions options) =>
        options.Concurrency == ConcurrencyControl.Exclusive || options.Isolation == IsolationLevel.Serializable ? []
        : options.Isolation == IsolationLevel.Snapshot ? [Anomaly.G2]
        : [Anomaly.GSingle, Anomaly.G2];

    /// <summary>Transactions committed and histories with G-single and with G2,
    /// pinned to the numbers the Rust and JS/WASM ports report for the same seeds.</summary>
    private static readonly Dictionary<string, (int, int, int)> Expected = new()
    {
        ["Optimistic ReadCommitted Revision"] = (847, 19, 0),
        ["Optimistic Snapshot Revision"] = (847, 0, 20),
        ["Optimistic Serializable Revision"] = (573, 0, 0),
        ["Optimistic Serializable ReadSet"] = (662, 0, 0),
        ["Pessimistic ReadCommitted Revision"] = (635, 17, 2),
        ["Pessimistic Snapshot Revision"] = (635, 0, 19),
        ["Pessimistic Serializable Revision"] = (453, 0, 0),
        ["Pessimistic Serializable ReadSet"] = (515, 0, 0),
        ["Exclusive ReadCommitted Revision"] = (148, 0, 0),
        ["Exclusive Snapshot Revision"] = (148, 0, 0),
        ["Exclusive Serializable Revision"] = (148, 0, 0),
        ["Exclusive Serializable ReadSet"] = (148, 0, 0),
    };

    [Theory]
    [MemberData(nameof(EngineInterleavingTests.Configurations), MemberType = typeof(EngineInterleavingTests))]
    public void LongRandomHistoriesShowOnlyTheAnomaliesEachIsolationLevelAllows(
        ConcurrencyControl concurrency, IsolationLevel isolation, SerializableValidation validation)
    {
        var options = new EngineOptions { Concurrency = concurrency, Isolation = isolation, SerializableValidation = validation };
        var context = $"{concurrency} {isolation} {validation}";
        var seen = new SortedDictionary<Anomaly, int>();
        var committed = 0;
        for (ulong seed = 1; seed <= Seeds; seed++)
        {
            var history = Generate(options, seed);
            committed += history.Transactions.Skip(1).Count(transaction => transaction.Revision is not null);
            foreach (var (anomaly, example) in Check(history))
            {
                Assert.True(Allowed(options).Contains(anomaly),
                    $"{context}, seed {seed}: {anomaly} through {string.Join("; ", example.Select(id => history.Transactions[id]))}");
                seen[anomaly] = seen.GetValueOrDefault(anomaly) + 1;
            }
        }
        const int attempted = Seeds * Transactions;
        output.WriteLine($"{context}: {committed} of {attempted} transactions committed, histories with anomalies {string.Join(", ", seen)}");
        // Refusing everything would pass the check above, so a share must
        // commit: a tenth under exclusive admission, a quarter otherwise.
        var share = concurrency == ConcurrencyControl.Exclusive ? 10 : 4;
        Assert.True(committed * share >= attempted, $"{context}: {committed} of {attempted} committed");
        // The weakest anomaly a level allows must show up.
        if (Allowed(options) is [var weakest, ..])
            Assert.True(seen.ContainsKey(weakest), $"{context}: the allowed {weakest} never occurred");
        Assert.Equal(Expected[context], (committed, seen.GetValueOrDefault(Anomaly.GSingle), seen.GetValueOrDefault(Anomaly.G2)));
    }

    [Fact]
    public void TheSameSeedReplaysTheSameHistory()
    {
        static string Text(History history) =>
            string.Join("\n", history.Transactions) + string.Join(",", history.FinalState);
        var options = new EngineOptions();
        Assert.Equal(Text(Generate(options, 7)), Text(Generate(options, 7)));
        Assert.NotEqual(Text(Generate(options, 7)), Text(Generate(options, 8)));
        // splitmix64's first output for seed 1, shared by every port.
        Assert.Equal(0x910A2DEC89025CC1UL, new Random(1).Next());
    }

    private static Transaction Committed(ulong revision, (uint, uint)[] reads, (uint, uint)[] writes, bool overlapping = false) => new()
    {
        Reads = reads.ToList(),
        Writes = writes.ToList(),
        Revision = revision,
        Began = overlapping ? 0 : (int)revision * 10,
        Ended = overlapping ? 1000 : (int)revision * 10 + 1,
    };

    private static Anomaly[] Kinds(Transaction[] transactions, params (uint, uint)[] final)
    {
        var all = new List<Transaction> { Committed(0, [], [(1, 1), (2, 2)]) };
        all.AddRange(transactions);
        return Check(new History(all, new SortedDictionary<uint, uint>(final.ToDictionary(row => row.Item1, row => row.Item2)))).Keys.ToArray();
    }

    [Fact]
    public void TheCheckerNamesEachAnomaly()
    {
        Assert.Empty(Kinds([Committed(1, [(1, 1)], [(1, 1000)]), Committed(2, [(1, 1000)], [(2, 1001)])], (1, 1000), (2, 1001)));
        // Write skew.
        Assert.Equal([Anomaly.G2], Kinds([Committed(1, [(1, 1), (2, 2)], [(1, 1000)], true), Committed(2, [(1, 1), (2, 2)], [(2, 1001)], true)], (1, 1000), (2, 1001)));
        // Lost update.
        Assert.Equal([Anomaly.GSingle], Kinds([Committed(1, [(1, 1)], [(1, 1000)], true), Committed(2, [(1, 1)], [(1, 1001)], true)], (1, 1001), (2, 2)));
        // Read skew.
        Assert.Equal([Anomaly.GSingle], Kinds([Committed(1, [], [(1, 1000), (2, 1001)], true), Committed(2, [(1, 1), (2, 1001)], [], true)], (1, 1000), (2, 1001)));
        // A stale read after the writer committed.
        Assert.Equal([Anomaly.GSingle], Kinds([Committed(1, [], [(1, 1000)]), Committed(2, [(1, 1)], [])], (1, 1000), (2, 2)));
        // Circular information flow.
        Assert.Equal([Anomaly.G1c], Kinds([Committed(1, [(2, 1001)], [(1, 1000)], true), Committed(2, [(1, 1000)], [(2, 1001)], true)], (1, 1000), (2, 1001)));
        // Aborted, intermediate and unknown values read.
        var aborted = Committed(1, [], [(1, 1000)]);
        aborted.Revision = null;
        Assert.Equal([Anomaly.Garbage, Anomaly.G1a, Anomaly.G1b],
            Kinds([aborted, Committed(2, [], [(2, 1001), (2, 1002)]), Committed(3, [(1, 1000), (2, 1001), (1, 5)], [])], (1, 1), (2, 1002)));
        // A lost committed write and a read that missed the transaction's own write.
        var own = Committed(1, [], [(1, 1000), (1, 1001)]);
        own.OwnReads.AddRange([(1, 1001, 1001), (1, 1000, 1001)]);
        Assert.Equal([Anomaly.Internal, Anomaly.LostWrite], Kinds([own], (1, 1), (2, 2)));
    }
}
