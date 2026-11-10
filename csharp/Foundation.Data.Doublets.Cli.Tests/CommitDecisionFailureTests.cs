using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class CommitDecisionFailureTests
{
    private static readonly string[] Points = ["marker-before", "marker-after", "log-before", "log-after"];
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
    private sealed class Log(string path, Fault fault) : NamedTypesDecorator<uint>(path)
    {
        public override string? GetName(uint link)
        {
            fault.Check("read-before");
            var name = base.GetName(link);
            fault.Check("read-after");
            return name;
        }
        public override uint SetName(uint link, string name)
        {
            var commit = name.StartsWith("__transactions:commit:", StringComparison.Ordinal);
            if (commit) fault.Check("marker-before");
            var result = base.SetName(link, name);
            if (commit && fault.Point == "marker-after")
            {
                // The marker really reaches the mapped journal before delivery fails.
                Flush();
                fault.Check("marker-after");
            }
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
                foreach (var operation in new[] { "commit", "async", "rollback", "dispose", "owner" }) cases.Add(point, false, operation);
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
    public static TheoryData<string, bool, string> ReadCases
    {
        get
        {
            var cases = new TheoryData<string, bool, string>();
            foreach (var point in Points)
                foreach (var automatic in new[] { false, true })
                    foreach (var read in new[] { "read-before", "read-after" }) cases.Add(point, automatic, read);
            return cases;
        }
    }
    [Theory]
    [MemberData(nameof(ReadCases))]
    public void FailedDecisionRereadRetainsEffectsAndAdmission(string point, bool automatic, string read)
    {
        Run(point, automatic, (tx, pending, fault, id) =>
        {
            fault.Point = read;
            Assert.Contains(read, Assert.Throws<IOException>(tx.Rollback).Message);
            Assert.Equal(21u, tx.GetSource(id));
            Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
            Assert.Throws<InvalidOperationException>(() => tx.SetName(id, "changed"));
            if (pending is not null) Assert.False(pending.IsCommitted);
            tx.Rollback();
            Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
        });
    }
    [Theory]
    [MemberData(nameof(CleanupCases))]
    public void CleanupResolvesPossiblyDurableDecisionBeforeFollowingEdit(string point, bool automatic, string operation)
    {
        Run(point, automatic, (tx, pending, fault, id) =>
        {
            void Cleanup()
            {
                if (operation == "commit") pending!.Commit();
                else if (operation == "async") pending!.CommitAsync().GetAwaiter().GetResult();
                else if (operation == "rollback") pending!.Rollback();
                else if (operation == "dispose") pending!.Dispose();
                else tx.Rollback();
            }
            // Even a second error must retain both the original effects and admission.
            fault.Point = "log-before";
            Assert.Contains("log-before", Assert.Throws<IOException>(Cleanup).Message);
            Assert.Equal(21u, tx.GetSource(id));
            Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
            if (pending is not null)
            {
                Assert.False(pending.IsCommitted);
                Assert.False(pending.IsRolledBack);
            }
            Cleanup();
            Assert.Equal(21u, tx.GetSource(id));
            Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
            if (pending is not null)
            {
                Assert.True(pending.IsCommitted);
                Assert.False(pending.IsRolledBack);
            }
        });
    }
    [Theory]
    [MemberData(nameof(WriteCases))]
    public void UncertainDecisionFencesEveryWrite(string point, bool automatic, string operation)
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
            Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
        });
    }
    private static void Run(string point, bool automatic, Action<TransactionsDecorator, ITransaction<uint>?, Fault, uint> verify)
    {
        var directory = Directory.CreateTempSubdirectory("commit-decision-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        try
        {
            uint id;
            var fault = new Fault();
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new Log(logPath, fault))
            using (var tx = new TransactionsDecorator(data, log, flushLog: () =>
            {
                fault.Check("log-before");
                log.Flush();
                fault.Check("log-after");
            }))
            {
                id = tx.CreateAndUpdate(11u, 12u);
                tx.SetName(id, "original");
                var decisions = Decisions(log);
                using var pending = automatic ? null : tx.BeginTransaction();
                if (!automatic) tx.Update(new[] { id }, new[] { id, 21u, 22u }, null);
                fault.Point = point;
                Assert.Contains(point, Assert.Throws<IOException>(() =>
                {
                    if (automatic) tx.Update(new[] { id }, new[] { id, 21u, 22u }, null);
                    else pending!.Commit();
                }).Message);
                Assert.Null(fault.Point);
                Assert.Equal(decisions + (point == "marker-before" ? 0 : 1), Decisions(log));
                if (pending is not null) Assert.False(pending.IsCommitted);
                Assert.Equal(21u, tx.GetSource(id));
                verify(tx, pending, fault, id);
                Assert.Equal(decisions + 1, Decisions(log));
            }
            for (var reopen = 0; reopen < 2; reopen++)
            {
                using var recoveredData = new NamedTypesDecorator<uint>(dataPath);
                using var recoveredLog = new NamedTypesDecorator<uint>(logPath);
                using var recovered = new TransactionsDecorator(recoveredData, recoveredLog);
                Assert.Equal(reopen == 0 ? 21u : 31u, recovered.GetSource(id));
                Assert.Equal(reopen == 0 ? 22u : 32u, recovered.GetTarget(id));
                Assert.Equal("original", recovered.GetName(id));
                Assert.Equal(recovered.LastLoggedSequence, recovered.AppliedSequence);
                if (reopen == 0) recovered.Update(new[] { id }, new[] { id, 31u, 32u }, null);
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
