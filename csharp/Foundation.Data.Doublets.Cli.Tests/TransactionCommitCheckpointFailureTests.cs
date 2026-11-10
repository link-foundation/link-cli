using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionCommitCheckpointFailureTests
{
    private static readonly string[] Points = ["data-before", "data-after", "marker-before", "marker-after", "log-before", "log-after"];
    private sealed class Fault
    {
        public string? Point;
        public bool Active, CheckpointCompleted;
        public int Flushes;
        public void Check(string point)
        {
            if (Point != point) return;
            Point = null;
            throw new IOException("injected " + point);
        }
    }
    private sealed class Log(string path, Fault fault) : NamedTypesDecorator<uint>(path)
    {
        public override uint SetName(uint link, string name)
        {
            var applied = name.StartsWith("__transactions:applied:", StringComparison.Ordinal);
            if (applied) fault.Check("marker-before");
            var result = base.SetName(link, name);
            if (applied) fault.Check("marker-after");
            return result;
        }
    }
    private static int Decisions(NamedTypesDecorator<uint> log)
    {
        var any = log.Constants.Any;
        return log.All(new Link<uint>(any, any, any)).Count(row => row is not null && log.GetName(row[0])?.StartsWith("__transactions:commit:", StringComparison.Ordinal) == true);
    }
    public static TheoryData<string, bool, string> CleanupCases
    {
        get
        {
            var cases = new TheoryData<string, bool, string>();
            foreach (var point in Points)
            {
                foreach (var cleanup in new[] { "commit", "async", "rollback", "dispose", "owner" }) cases.Add(point, false, cleanup);
                cases.Add(point, true, "owner");
            }
            return cases;
        }
    }
    public static TheoryData<string, bool, string> WriteCases
    {
        get
        {
            var cases = new TheoryData<string, bool, string>();
            foreach (var point in Points)
                foreach (var automatic in new[] { false, true })
                    foreach (var operation in new[] { "create", "update", "delete", "set", "remove" }) cases.Add(point, automatic, operation);
            return cases;
        }
    }
    [Theory]
    [MemberData(nameof(CleanupCases))]
    public void RetryFinishesKnownCommitBeforeFollowingEdit(string point, bool automatic, string cleanup)
    {
        Run(point, automatic, (tx, pending, fault, id) =>
        {
            void Cleanup()
            {
                if (cleanup == "commit") pending!.Commit();
                else if (cleanup == "async") pending!.CommitAsync().GetAwaiter().GetResult();
                else if (cleanup == "rollback") pending!.Rollback();
                else if (cleanup == "dispose") pending!.Dispose();
                else tx.Rollback();
            }
            fault.Point = point;
            Assert.Contains(point, Assert.Throws<IOException>(Cleanup).Message);
            Assert.False(fault.CheckpointCompleted);
            Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
            Assert.Equal(21u, tx.GetSource(id));
            Cleanup();
            Assert.True(fault.CheckpointCompleted, "Cleanup must finish the committed checkpoint.");
            Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
        });
    }
    [Theory]
    [MemberData(nameof(WriteCases))]
    public void WritesWaitForKnownCommitCheckpoint(string point, bool automatic, string operation)
    {
        Run(point, automatic, (tx, pending, fault, id) =>
        {
            var sequence = tx.LastLoggedSequence;
            Assert.Throws<InvalidOperationException>(() =>
            {
                if (operation == "create") tx.Create(null, null);
                else if (operation == "update") tx.Update(new[] { id }, new[] { id, 51u, 52u }, null);
                else if (operation == "delete") tx.Delete(new[] { id }, null);
                else if (operation == "set") tx.SetName(id, "changed");
                else tx.RemoveName(id);
            });
            Assert.Equal(sequence, tx.LastLoggedSequence);
            Assert.Equal(21u, tx.GetSource(id));
            Assert.Equal("original", tx.GetName(id));
            Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
            Assert.Throws<InvalidOperationException>(tx.Recover);
            tx.Rollback();
            Assert.True(fault.CheckpointCompleted);
        });
    }
    private static void Run(string point, bool automatic, Action<TransactionsDecorator, ITransaction<uint>?, Fault, uint> verify)
    {
        var directory = Directory.CreateTempSubdirectory("commit-checkpoint-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        try
        {
            uint id;
            var fault = new Fault();
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new Log(logPath, fault))
            using (var tx = new TransactionsDecorator(data, log,
                flushData: () => { fault.Check("data-before"); data.Flush(); fault.Check("data-after"); },
                flushLog: () =>
                {
                    var checkpoint = fault.Active && ++fault.Flushes > 1;
                    if (checkpoint) fault.Check("log-before");
                    log.Flush();
                    if (checkpoint)
                    {
                        fault.Check("log-after");
                        fault.CheckpointCompleted = true;
                    }
                }))
            {
                id = tx.CreateAndUpdate(11u, 12u);
                tx.SetName(id, "original");
                var decisions = Decisions(log);
                using var pending = automatic ? null : tx.BeginTransaction();
                if (!automatic) tx.Update(new[] { id }, new[] { id, 21u, 22u }, null);
                fault.Point = point; fault.Active = true;
                Assert.Contains(point, Assert.Throws<IOException>(() =>
                {
                    if (automatic) tx.Update(new[] { id }, new[] { id, 21u, 22u }, null);
                    else pending!.Commit();
                }).Message);
                Assert.False(fault.CheckpointCompleted);
                Assert.Equal(decisions + 1, Decisions(log));
                if (pending is not null) Assert.True(pending.IsCommitted);
                Assert.Equal(21u, tx.GetSource(id));
                verify(tx, pending, fault, id);
                Assert.Equal(decisions + 1, Decisions(log));
                using (var next = tx.BeginTransaction()) next.Rollback();
                tx.Update(new[] { id }, new[] { id, 31u, 32u }, null);
            }
            using var recoveredData = new NamedTypesDecorator<uint>(dataPath);
            using var recoveredLog = new NamedTypesDecorator<uint>(logPath);
            using var recovered = new TransactionsDecorator(recoveredData, recoveredLog);
            Assert.Equal(31u, recovered.GetSource(id));
            Assert.Equal(32u, recovered.GetTarget(id));
            Assert.Equal("original", recovered.GetName(id));
            Assert.Equal(recovered.LastLoggedSequence, recovered.AppliedSequence);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
