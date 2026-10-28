using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class AutomaticVersionFailureTests
{
    private sealed class Fault
    {
        public string? Point;
        public int Flushes;
        public void Check(string point)
        {
            if (Point != point) return;
            Point = null;
            throw new IOException("injected " + point);
        }
    }
    private sealed class Store(string path, Fault fault) : NamedTypesDecorator<uint>(path)
    {
        public override uint SetName(uint link, string name)
        {
            var phase = name.StartsWith("__transactions:commit:", StringComparison.Ordinal) ? "decision"
                : name.StartsWith("__transactions:applied:", StringComparison.Ordinal) ? "applied"
                : name.StartsWith("__vc:trans:", StringComparison.Ordinal) ? "vc-transition"
                : name.StartsWith("__vc:branch:", StringComparison.Ordinal) ? "vc-head"
                : name.StartsWith("__vc:applied=", StringComparison.Ordinal) ? "vc-applied" : null;
            if (phase is not null) fault.Check(phase + "-before");
            var result = base.SetName(link, name);
            if (phase == "decision" && fault.Point == "decision-after") Flush();
            if (phase is not null) fault.Check(phase + "-after");
            return result;
        }
    }
    public static TheoryData<string, string> Cases
    {
        get
        {
            var cases = new TheoryData<string, string>();
            foreach (var phase in new[] { "decision", "decision-log", "data", "applied", "checkpoint-log", "vc-transition", "vc-head", "vc-applied" })
                foreach (var edge in new[] { "before", "after" })
                    foreach (var entry in new[] { "owner", "graph" }) cases.Add(phase + "-" + edge, entry);
            return cases;
        }
    }
    [Theory]
    [MemberData(nameof(Cases))]
    public void AutomaticFailureRetainsVersionPublicationThroughRepeatedCleanup(string point, string entry)
    {
        var directory = Directory.CreateTempSubdirectory("automatic-version-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        var branchPath = Path.Combine(directory, "branches.links");
        try
        {
            uint id;
            long originalHead, expected;
            var fault = new Fault();
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new Store(logPath, fault))
            using (var branches = new Store(branchPath, fault))
            using (var tx = new TransactionsDecorator(data, log,
                flushData: () => { fault.Check("data-before"); data.Flush(); fault.Check("data-after"); },
                flushLog: () =>
                {
                    var phase = ++fault.Flushes == 1 ? "decision-log" : "checkpoint-log";
                    fault.Check(phase + "-before"); log.Flush(); fault.Check(phase + "-after");
                }))
            using (var vc = new VersionControlDecorator(tx, branches))
            {
                id = vc.CreateAndUpdate(11u, 12u);
                vc.SetName(id, "original");
                originalHead = vc.CurrentSequence;
                vc.Branch("topic"); vc.SwitchBranch("topic");
                fault.Flushes = 0;
                fault.Point = point;
                Assert.Contains(point, Assert.Throws<IOException>(() => vc.Update(new[] { id }, new[] { id, 21u, 22u }, null)).Message);
                Assert.Null(fault.Point);
                expected = tx.LastLoggedSequence;
                Assert.True(expected > originalHead);
                Assert.Equal(21u, vc.GetSource(id));
                void Fences()
                {
                    Assert.Throws<InvalidOperationException>(() => vc.CreateAndUpdate(51u, 52u));
                    Assert.Throws<InvalidOperationException>(() => vc.Update(new[] { id }, new[] { id, 51u, 52u }, null));
                    Assert.Throws<InvalidOperationException>(() => vc.Delete(new[] { id }, null));
                    Assert.Throws<InvalidOperationException>(() => vc.SetName(id, "changed"));
                    Assert.Throws<InvalidOperationException>(() => vc.RemoveName(id));
                    Assert.Throws<InvalidOperationException>(() => vc.BeginTransaction());
                    Assert.Throws<InvalidOperationException>(() => vc.Branch("blocked"));
                    Assert.Throws<InvalidOperationException>(() => vc.SwitchBranch("main"));
                    Assert.Throws<InvalidOperationException>(() => vc.Tag("blocked"));
                    Assert.Throws<InvalidOperationException>(() => vc.Checkout(0));
                    Assert.Throws<InvalidOperationException>(vc.Recover);
                    Assert.Equal("original", vc.GetName(id));
                    Assert.Equal(expected, tx.LastLoggedSequence);
                }
                Fences();
                void Cleanup()
                {
                    if (entry == "owner") vc.Dispose();
                    else LinksFacadeDisposer.Dispose(vc);
                }
                fault.Point = "vc-head-before";
                Assert.Contains("vc-head-before", Assert.Throws<IOException>(Cleanup).Message);
                Assert.Equal(21u, vc.GetSource(id));
                Fences();
                Cleanup();
                Assert.Equal(expected, vc.CurrentSequence);
                Assert.Equal(expected, vc.ListBranches().Single(b => b.Name == "topic").Head);
                Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
                if (entry != "graph") LinksFacadeDisposer.Dispose(vc);
            }
            for (var reopen = 0; reopen < 2; reopen++)
            {
                using var data = new NamedTypesDecorator<uint>(dataPath);
                using var log = new NamedTypesDecorator<uint>(logPath);
                using var branches = new NamedTypesDecorator<uint>(branchPath);
                using var tx = new TransactionsDecorator(data, log);
                using var vc = new VersionControlDecorator(tx, branches);
                Assert.Equal("topic", vc.CurrentBranch);
                Assert.Equal(reopen == 0 ? expected : expected + 1, vc.CurrentSequence);
                Assert.Equal(reopen == 0 ? 21u : 31u, vc.GetSource(id));
                Assert.Equal("original", vc.GetName(id));
                vc.Checkout(originalHead);
                Assert.Equal(11u, vc.GetSource(id));
                vc.Checkout(expected);
                Assert.Equal(21u, vc.GetSource(id));
                if (reopen == 0)
                {
                    using var next = vc.BeginTransaction();
                    vc.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                    next.Commit();
                }
                if (reopen == 1) vc.Checkout(expected + 1);
                data.Flush(); log.Flush(); branches.Flush();
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
