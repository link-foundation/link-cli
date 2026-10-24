using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Tests;

public class RecoveryInterruptionTests
{
    private sealed class Fault
    {
        public string? Point;
        public void Check(string point)
        {
            if (Point != point) return;
            Point = null;
            throw new IOException("injected " + point);
        }
    }
    private sealed class Store(string path, Fault fault, string operation) : NamedTypesDecorator<uint>(path)
    {
        public override uint Update(IList<uint>? restriction, IList<uint>? substitution, WriteHandler<uint>? handler)
        {
            var phase = substitution?[0] == 2u ? "undo" : substitution?[1] == 21u ? "old" : "new";
            fault.Check(phase + "-before");
            var result = base.Update(restriction, substitution, handler);
            fault.Check(phase + "-after");
            return result;
        }
        public override uint Delete(IList<uint>? restriction, WriteHandler<uint>? handler)
        {
            var phase = operation == "recreate" ? "old" : "new";
            fault.Check(phase + "-before");
            var result = base.Delete(restriction, handler);
            fault.Check(phase + "-after");
            return result;
        }
    }
    private sealed class Log(string path, Fault fault) : NamedTypesDecorator<uint>(path)
    {
        public override uint SetName(uint link, string name)
        {
            var anchor = name == "__transactions:applied:2";
            if (anchor) fault.Check("marker-before");
            var result = base.SetName(link, name);
            if (anchor) fault.Check("marker-after");
            return result;
        }
    }
    public static TheoryData<string, string> Cases
    {
        get
        {
            var cases = new TheoryData<string, string>();
            foreach (var operation in new[] { "update", "delete", "recreate" })
                foreach (var point in new[] { "old-before", "old-after", "new-before", "new-after",
                    "undo-before", "undo-after", "data-before", "data-after", "marker-before", "marker-after", "log-before", "log-after" })
                    cases.Add(operation, point);
            return cases;
        }
    }
    private static void Seed(NamedTypesDecorator<uint> data, NamedTypesDecorator<uint> log, string operation)
    {
        data.EnsureCreated(1u, 2u);
        Assert.True(data.Exists(1u));
        Assert.True(data.Exists(2u));
        data.Update(new[] { 2u }, new[] { 2u, 51u, 52u }, null);
        if (operation == "delete") data.Delete(new[] { 1u }, null);
        else data.Update(new[] { 1u }, new[] { 1u, 31u, 32u }, null);
        var empty = new Link<uint>(0, 0, 0);
        Link<uint> State(uint source, uint target) => new(1u, source, target);
        var olderKind = operation == "recreate" ? TransitionKind.Delete : TransitionKind.Update;
        var olderAfter = operation == "recreate" ? empty : State(21, 22);
        var newerKind = operation == "delete" ? TransitionKind.Delete : operation == "recreate" ? TransitionKind.Create : TransitionKind.Update;
        var newerBefore = operation == "recreate" ? empty : State(21, 22);
        var newerAfter = operation == "delete" ? empty : State(31, 32);
        var changes = new[] {
            (TransitionKind.Create, empty, State(11, 12)),
            (olderKind, State(11, 12), olderAfter),
            (newerKind, newerBefore, newerAfter),
            (TransitionKind.Update, new Link<uint>(2, 41, 42), new Link<uint>(2, 51, 52)) };
        var markerIds = new HashSet<uint>();
        void Write(string name)
        {
            var marker = log.CreateAndUpdate(0u, 0u);
            Assert.NotEqual(0u, marker);
            Assert.True(markerIds.Add(marker));
            log.SetName(marker, name);
            Assert.Equal(name, log.GetName(marker));
        }
        for (var index = 0; index < changes.Length; index++)
        {
            var sequence = index + 1;
            var id = Guid.ParseExact(sequence.ToString("x32"), "N");
            var (kind, before, after) = changes[index];
            var record = new Transition<uint>(id, sequence, DateTimeOffset.UnixEpoch, kind, before, after);
            Write("__transactions:transition:" + record.Serialize());
            if (sequence != 4) Write("__transactions:commit:" + id.ToString("N"));
            if (sequence is 1 or 3) Write("__transactions:applied:" + sequence);
        }
        data.Flush(); log.Flush();
    }
    [Theory(Timeout = 15000)]
    [MemberData(nameof(Cases))]
    public async Task ReopenAfterInterruptedRecoveryPreservesLatestCommitAndPendingUndo(string operation, string point)
    {
        await Task.CompletedTask;
        var directory = Directory.CreateTempSubdirectory("recovery-interrupted-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        try
        {
            var fault = new Fault();
            using (var data = new Store(dataPath, fault, operation))
            using (var log = new Log(logPath, fault))
            {
                Seed(data, log, operation);
                fault.Point = point;
                Assert.Contains(point, Assert.Throws<IOException>(() => new TransactionsDecorator(data, log,
                    flushData: () => { fault.Check("data-before"); data.Flush(); fault.Check("data-after"); },
                    flushLog: () => { fault.Check("log-before"); log.Flush(); fault.Check("log-after"); })).Message);
                Assert.Null(fault.Point);
                if (point.StartsWith("old-", StringComparison.Ordinal) || point.StartsWith("new-", StringComparison.Ordinal)
                    || point.StartsWith("undo-", StringComparison.Ordinal) || point == "data-before")
                {
                    var any = log.Constants.Any;
                    Assert.DoesNotContain(log.All(new Link<uint>(any, any, any)), row => row is not null && log.GetName(row[0]) == "__transactions:applied:2");
                }
            }
            for (var reopen = 0; reopen < 2; reopen++)
            {
                using var data = new NamedTypesDecorator<uint>(dataPath);
                using var log = new NamedTypesDecorator<uint>(logPath);
                using var recovered = new TransactionsDecorator(data, log);
                if (operation == "delete") Assert.False(recovered.Exists(1u));
                else
                {
                    Assert.True(recovered.Exists(1u));
                    Assert.Equal(31u, recovered.GetSource(1u));
                    Assert.Equal(32u, recovered.GetTarget(1u));
                }
                Assert.Equal(41u, recovered.GetSource(2u));
                Assert.Equal(42u, recovered.GetTarget(2u));
                Assert.Equal(3, recovered.AppliedSequence);
                Assert.Equal(4, recovered.LastLoggedSequence);
                var any = log.Constants.Any;
                Assert.Contains(log.All(new Link<uint>(any, any, any)), row => row is not null && log.GetName(row[0]) == "__transactions:applied:2");
                Assert.Contains(log.All(new Link<uint>(any, any, any)), row => row is not null && log.GetName(row[0]) == "__transactions:rollback:00000000000000000000000000000004");
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
