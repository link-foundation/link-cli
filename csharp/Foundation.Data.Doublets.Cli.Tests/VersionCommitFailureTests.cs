using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionCommitFailureTests
{
    [Theory]
    [InlineData("set")]
    [InlineData("remove")]
    public void NamedWritesWaitForCommittedVersionPublication(string operation)
    {
        var directory = Directory.CreateTempSubdirectory("version-name-fence-").FullName;
        try
        {
            var fault = new Fault();
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
            using var tx = new TransactionsDecorator(data, log,
                flushData: () => { data.Flush(); fault.Check("data-after"); });
            using var vc = new VersionControlDecorator(tx, branches);
            var baseline = vc.CreateAndUpdate(11u, 12u);
            vc.SetName(baseline, "original");
            using var pending = vc.BeginTransaction();
            vc.CreateAndUpdate(21u, 22u);
            fault.Point = "data-after";
            Assert.Throws<IOException>(pending.Commit);
            Assert.True(pending.IsCommitted);
            var sequence = tx.LastLoggedSequence;
            if (operation == "set")
                Assert.Throws<InvalidOperationException>(() => vc.SetName(baseline, "changed"));
            else
                Assert.Throws<InvalidOperationException>(() => vc.RemoveName(baseline));
            Assert.Equal("original", vc.GetName(baseline));
            Assert.Equal(baseline, vc.GetByName("original"));
            Assert.Equal(sequence, tx.LastLoggedSequence);
            pending.Dispose();
            vc.SetName(baseline, "changed");
            Assert.Equal("changed", vc.GetName(baseline));
            vc.RemoveName(baseline);
            Assert.Null(vc.GetName(baseline));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

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

    private sealed class Store(string path, Fault fault) : NamedTypesDecorator<uint>(path)
    {
        public override uint SetName(uint link, string name)
        {
            var applied = name.StartsWith("__transactions:applied:", StringComparison.Ordinal);
            var metadata = name.StartsWith("__vc:trans:", StringComparison.Ordinal) ? "vc-transition"
                : name.StartsWith("__vc:branch:", StringComparison.Ordinal) ? "vc-head"
                : name.StartsWith("__vc:applied=", StringComparison.Ordinal) ? "vc-applied" : null;
            if (applied) fault.Check("marker-before");
            if (metadata is not null) fault.Check(metadata + "-before");
            var result = base.SetName(link, name);
            if (applied) fault.Check("marker-after");
            if (metadata is not null) fault.Check(metadata + "-after");
            return result;
        }
    }

    public static TheoryData<string, string> FailureCases
    {
        get
        {
            var cases = new TheoryData<string, string>();
            foreach (var point in new[] { "data-before", "data-after", "marker-before", "marker-after",
                "log-before", "log-after", "vc-transition-before", "vc-transition-after",
                "vc-head-before", "vc-head-after", "vc-applied-before", "vc-applied-after" })
                foreach (var entry in new[] { "transaction", "owner", "graph", "rollback" }) cases.Add(point, entry);
            return cases;
        }
    }

    [Theory]
    [MemberData(nameof(FailureCases))]
    public void RetryPublishesCommittedVersionAfterCheckpointFailure(string point, string entry)
    {
        var directory = Directory.CreateTempSubdirectory("version-commit-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        var branchPath = Path.Combine(directory, "branches.links");
        try
        {
            uint baseline, created; long originalHead, committedHead;
            var fault = new Fault(); var commitFlushes = 0;
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new Store(logPath, fault))
            using (var branches = new Store(branchPath, fault))
            using (var tx = new TransactionsDecorator(data, log,
                flushData: () => { fault.Check("data-before"); data.Flush(); fault.Check("data-after"); },
                flushLog: () =>
                {
                    // The first armed flush makes the commit decision durable;
                    // inject only into the later applied-marker checkpoint.
                    var checkpoint = fault.Point is "log-before" or "log-after" && ++commitFlushes > 1;
                    if (checkpoint) fault.Check("log-before");
                    log.Flush();
                    if (checkpoint) fault.Check("log-after");
                }))
            using (var vc = new VersionControlDecorator(tx, branches))
            {
                baseline = vc.CreateAndUpdate(11u, 12u);
                originalHead = vc.CurrentSequence;
                vc.Branch("topic"); vc.SwitchBranch("topic");
                using var pending = vc.BeginTransaction();
                created = vc.CreateAndUpdate(21u, 22u);
                committedHead = tx.LastLoggedSequence;
                fault.Point = point;
                Assert.Contains(point, Assert.Throws<IOException>(pending.Commit).Message);
                Assert.True(pending.IsCommitted); Assert.False(pending.IsRolledBack);
                Assert.Throws<InvalidOperationException>(() => vc.CreateAndUpdate(91u, 92u));
                Assert.Throws<InvalidOperationException>(() => vc.Branch("blocked"));
                Assert.Throws<InvalidOperationException>(() => vc.SwitchBranch("main"));
                Assert.Throws<InvalidOperationException>(() => vc.Tag("blocked"));
                Assert.Throws<InvalidOperationException>(() => vc.Checkout(0));
                Assert.Throws<InvalidOperationException>(() => vc.BeginTransaction());
                if (entry == "transaction") pending.Dispose();
                else if (entry == "owner") vc.Dispose();
                else if (entry == "rollback") pending.Rollback();
                else LinksFacadeDisposer.Dispose(vc);
                Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
                Assert.Equal(committedHead, vc.CurrentSequence);
                Assert.Equal(committedHead, vc.ListBranches().Single(b => b.Name == "topic").Head);
                if (entry != "graph")
                {
                    vc.Tag("committed");
                    LinksFacadeDisposer.Dispose(vc);
                }
            }
            foreach (var path in Directory.EnumerateFiles(directory))
            {
                using var exclusive = new FileStream(path, FileMode.Open, FileAccess.ReadWrite, FileShare.None);
                Assert.True(exclusive.CanRead);
            }
            using var recoveredData = new NamedTypesDecorator<uint>(dataPath);
            using var recoveredLog = new NamedTypesDecorator<uint>(logPath);
            using var recoveredBranches = new NamedTypesDecorator<uint>(branchPath);
            using var recoveredTx = new TransactionsDecorator(recoveredData, recoveredLog);
            using var recovered = new VersionControlDecorator(recoveredTx, recoveredBranches);
            Assert.Equal("topic", recovered.CurrentBranch);
            Assert.Equal(committedHead, recovered.CurrentSequence);
            var any = recoveredBranches.Constants.Any;
            var markers = recoveredBranches.All(new[] { any, any, any })
                .Select(row => recoveredBranches.GetName(Assert.IsAssignableFrom<IList<uint>>(row)[0]))
                .Where(name => name is not null && name.StartsWith("__vc:trans:", StringComparison.Ordinal))
                .ToArray();
            Assert.Equal(committedHead, (long)markers.Length);
            Assert.Equal(markers.Length, markers.Distinct(StringComparer.Ordinal).Count());
            Assert.Equal(2u, recovered.Count());
            Assert.Equal(11u, recovered.GetSource(baseline)); Assert.Equal(12u, recovered.GetTarget(baseline));
            Assert.Equal(21u, recovered.GetSource(created)); Assert.Equal(22u, recovered.GetTarget(created));
            recovered.Checkout(originalHead);
            Assert.Equal(1u, recovered.Count()); Assert.True(recovered.Exists(baseline));
            recovered.Checkout(committedHead);
            Assert.Equal(2u, recovered.Count()); Assert.True(recovered.Exists(created));
            using (var next = recovered.BeginTransaction())
            { recovered.CreateAndUpdate(31u, 32u); next.Commit(); }
            Assert.True(recovered.CurrentSequence > committedHead);
            recovered.Tag("following");
            Assert.True(recovered.TryGetTag("following", out var sequence));
            Assert.Equal(recovered.CurrentSequence, sequence);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
