using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>Every interleaving of two small transactions, checked for
/// serializability, as in PostgreSQL's isolation tester (which runs "all possible
/// interleavings" of the sessions' steps) and Jepsen's Elle (which judges a
/// history from client observations). Mirrors rust/tests/engine_interleaving_tests.rs.</summary>
public class EngineInterleavingTests
{
    private abstract record Step;
    private sealed record Read(uint Id) : Step;
    private sealed record Lock(uint Id) : Step;
    private sealed record Scan : Step;
    private sealed record Write(uint Id) : Step;
    private sealed record Commit : Step;

    private sealed record Workload(string Name, Step[][] Transactions);
    private static readonly (uint Id, uint Target)[] Initial = { (1, 3), (2, 4) };
    private static readonly Workload LostUpdate = new("lost update",
        new[] { new Step[] { new Read(1), new Write(1), new Commit() }, new Step[] { new Read(1), new Write(1), new Commit() } });
    private static readonly Workload ReadSkew = new("read skew",
        new[] { new Step[] { new Read(1), new Read(2), new Commit() }, new Step[] { new Write(1), new Write(2), new Commit() } });
    private static readonly Workload WriteSkew = new("write skew",
        new[] { new Step[] { new Read(1), new Read(2), new Write(1), new Commit() }, new Step[] { new Read(1), new Read(2), new Write(2), new Commit() } });
    private static readonly Workload ScanSkew = new("write skew through a scan",
        new[] { new Step[] { new Scan(), new Write(1), new Commit() }, new Step[] { new Scan(), new Write(2), new Commit() } });
    private static readonly Workload LockedWriteSkew = new("write skew with locked reads",
        new[] { new Step[] { new Lock(1), new Lock(2), new Write(1), new Commit() }, new Step[] { new Lock(1), new Lock(2), new Write(2), new Commit() } });
    private static readonly Workload Disjoint = new("disjoint read and write",
        new[] { new Step[] { new Read(1), new Write(1), new Commit() }, new Step[] { new Read(2), new Write(2), new Commit() } });

    private sealed class Run
    {
        public EngineSession<uint>? Session;
        public readonly List<uint> Reads = new();
        public bool Aborted, Committed;
    }

    private static void Execute(LinksEngine<uint> engine, Run run, Step step)
    {
        var session = run.Session ??= engine.Begin();
        switch (step)
        {
            case Read read: run.Reads.Add(session.GetLink(read.Id)!.Value.Target); break;
            case Lock locked: run.Reads.Add(session.LockForUpdate(locked.Id)!.Value.Target); break;
            case Scan:
                uint sum = 0;
                session.Each(null, link => { sum += link![2]; return session.Constants.Continue; });
                run.Reads.Add(sum);
                break;
            case Write write: session.UpdateLink(write.Id, write.Id, 1 + (uint)run.Reads.Sum(value => (long)value)); break;
            case Commit: session.Commit(); run.Committed = true; break;
        }
    }

    /// <summary>The reads each transaction makes, and the final state, when they
    /// run one after another from the initial state.</summary>
    private static (List<List<uint>> Reads, SortedDictionary<uint, uint> State) Serial(IEnumerable<Step[]> order)
    {
        var state = new SortedDictionary<uint, uint>(Initial.ToDictionary(row => row.Id, row => row.Target));
        var all = new List<List<uint>>();
        foreach (var steps in order)
        {
            var reads = new List<uint>();
            foreach (var step in steps)
                switch (step)
                {
                    case Read read: reads.Add(state[read.Id]); break;
                    case Lock locked: reads.Add(state[locked.Id]); break;
                    case Scan: reads.Add((uint)state.Values.Sum(value => (long)value)); break;
                    case Write write: state[write.Id] = 1 + (uint)reads.Sum(value => (long)value); break;
                }
            all.Add(reads);
        }
        return (all, state);
    }

    /// <summary>Every order of two transactions' steps that keeps each one's own order.</summary>
    private static List<int[]> Interleavings(int first, int second)
    {
        if (first == 0 || second == 0)
            return new() { Enumerable.Repeat(0, first).Concat(Enumerable.Repeat(1, second)).ToArray() };
        return Interleavings(first - 1, second).Select(tail => tail.Prepend(0).ToArray())
            .Concat(Interleavings(first, second - 1).Select(tail => tail.Prepend(1).ToArray())).ToList();
    }

    private static (bool Serializable, bool BothCommitted) Execute(EngineOptions options, Workload workload, int[] schedule)
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog(), options);
        using (var seed = engine.Begin())
        {
            foreach (var (id, target) in Initial) { seed.EnsureCreated(id); seed.UpdateLink(id, id, target); }
            seed.Commit();
        }
        var runs = new[] { new Run(), new Run() };
        var next = new int[2];
        foreach (var who in schedule)
        {
            var step = workload.Transactions[who][next[who]++];
            var run = runs[who];
            if (run.Aborted) continue;
            try { Execute(engine, run, step); }
            catch (InvalidOperationException)
            {
                // A refused step ends the transaction, as a client would roll back.
                run.Aborted = true;
                run.Session?.Dispose();
                run.Session = null;
            }
        }
        foreach (var run in runs) run.Session?.Dispose();
        var committed = Enumerable.Range(0, 2).Where(who => runs[who].Committed).ToArray();
        var actual = new SortedDictionary<uint, uint>(engine.Snapshot().Rows.ToDictionary(row => row.Key, row => row.Value.Link.Target));
        var orders = committed.Length == 2 ? new[] { committed, committed.Reverse().ToArray() } : new[] { committed };
        var serializable = orders.Any(order =>
        {
            var (reads, state) = Serial(order.Select(who => workload.Transactions[who]));
            return state.SequenceEqual(actual) && order.Zip(reads).All(pair => runs[pair.First].Reads.SequenceEqual(pair.Second));
        });
        return (serializable, committed.Length == 2);
    }

    /// <summary>Whether the configuration must refuse every non-serializable
    /// outcome of the workload. Where it need not, the anomaly must occur.</summary>
    private static bool Prevents(EngineOptions options, Workload workload) =>
        options.Concurrency == ConcurrencyControl.Exclusive || options.Isolation == IsolationLevel.Serializable ||
        workload == LostUpdate || workload == LockedWriteSkew || workload == Disjoint ||
        workload == ReadSkew && options.Isolation == IsolationLevel.Snapshot;

    public static TheoryData<ConcurrencyControl, IsolationLevel, SerializableValidation> Configurations()
    {
        var data = new TheoryData<ConcurrencyControl, IsolationLevel, SerializableValidation>();
        foreach (var concurrency in new[] { ConcurrencyControl.Optimistic, ConcurrencyControl.Pessimistic, ConcurrencyControl.Exclusive })
        {
            data.Add(concurrency, IsolationLevel.ReadCommitted, SerializableValidation.Revision);
            data.Add(concurrency, IsolationLevel.Snapshot, SerializableValidation.Revision);
            data.Add(concurrency, IsolationLevel.Serializable, SerializableValidation.Revision);
            data.Add(concurrency, IsolationLevel.Serializable, SerializableValidation.ReadSet);
        }
        return data;
    }

    [Theory]
    [MemberData(nameof(Configurations))]
    public void EveryInterleavingIsSerializableExactlyWhereTheIsolationLevelPromises(
        ConcurrencyControl concurrency, IsolationLevel isolation, SerializableValidation validation)
    {
        var options = new EngineOptions { Concurrency = concurrency, Isolation = isolation, SerializableValidation = validation };
        foreach (var workload in new[] { LostUpdate, ReadSkew, WriteSkew, ScanSkew, LockedWriteSkew, Disjoint })
        {
            if (workload == LockedWriteSkew && concurrency != ConcurrencyControl.Pessimistic) continue;
            var outcomes = Interleavings(workload.Transactions[0].Length, workload.Transactions[1].Length)
                .Select(schedule => (Schedule: schedule, Outcome: Execute(options, workload, schedule))).ToList();
            var context = $"{workload.Name} under {concurrency} {isolation} {validation}";
            Assert.True(outcomes.Any(o => o.Outcome.BothCommitted), $"{context}: no interleaving committed both");
            var anomaly = outcomes.FirstOrDefault(o => !o.Outcome.Serializable).Schedule;
            if (Prevents(options, workload))
                Assert.True(anomaly is null, $"{context}: non-serializable interleaving {string.Join(",", anomaly ?? [])}");
            else
                Assert.True(anomaly is not null, $"{context}: the allowed anomaly never occurred");
            if (workload == Disjoint && concurrency != ConcurrencyControl.Exclusive &&
                (isolation != IsolationLevel.Serializable || validation == SerializableValidation.ReadSet))
                Assert.True(outcomes.All(o => o.Outcome.BothCommitted), $"{context}: refused independent work");
        }
    }

    [Fact]
    public void TheCheckerRecognizesSerialAndAnomalousHistories()
    {
        Assert.Equal(new[] { new[] { 0, 0, 1 }, new[] { 0, 1, 0 }, new[] { 1, 0, 0 } }, Interleavings(2, 1));
        Assert.Equal(70, Interleavings(4, 4).Count);
        var snapshot = new EngineOptions { Isolation = IsolationLevel.Snapshot };
        Assert.Equal((true, true), Execute(snapshot, WriteSkew, new[] { 0, 0, 0, 0, 1, 1, 1, 1 }));
        // Both read before either writes: each takes one doctor off call.
        Assert.Equal((false, true), Execute(snapshot, WriteSkew, new[] { 0, 0, 1, 1, 0, 0, 1, 1 }));
    }
}
