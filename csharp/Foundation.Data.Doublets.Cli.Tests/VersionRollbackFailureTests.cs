using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionRollbackFailureTests
{
    [Fact]
    public void RetentionFailureAfterTerminalRollbackReleasesVersionAdmission()
    {
        var directory = Directory.CreateTempSubdirectory("version-terminal-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
            var remaining = 0;
            void Flush()
            {
                if (remaining > 0 && --remaining == 0) throw new IOException("injected checkpoint");
                data.Flush();
            }
            using var tx = new TransactionsDecorator(data, log, flushData: Flush, flushLog: log.Flush);
            // Retain one committed transition whose checkpoint must be retried.
            // Construct the version facade after that failure so this control
            // isolates rollback admission from commit attribution.
            remaining = 1;
            Assert.Throws<IOException>(() => tx.CreateAndUpdate(11u, 12u));
            tx.RetentionPolicy = new LogRetentionPolicy.Sized(1);
            using var vc = new VersionControlDecorator(tx, branches);
            using var pending = vc.BeginTransaction();
            vc.CreateAndUpdate(21u, 22u);
            // Rollback's own flush succeeds; retention's older checkpoint fails
            // after MarkRolledBack has already released the inner owner.
            remaining = 2;
            Assert.Contains("checkpoint", Assert.Throws<IOException>(pending.Rollback).Message);
            Assert.True(pending.IsRolledBack);
            Assert.Equal(1u, vc.Count());
            Assert.Equal(0, tx.AppliedSequence);
            vc.Dispose();
            vc.Branch("following"); vc.SwitchBranch("following");
            using var next = vc.BeginTransaction();
            vc.CreateAndUpdate(31u, 32u); next.Commit();
            vc.Tag("following");
            Assert.True(vc.TryGetTag("following", out var sequence));
            Assert.Equal(vc.CurrentSequence, sequence);
            Assert.True(sequence > 0);
            Assert.Equal(2u, vc.Count());
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
        public override uint Update(IList<uint>? restriction, IList<uint>? substitution, WriteHandler<uint>? handler)
        {
            fault.Check("undo-before");
            var result = base.Update(restriction, substitution, handler);
            fault.Check("undo-after");
            return result;
        }
        public override uint SetName(uint link, string name)
        {
            var rollback = name.StartsWith("__transactions:rollback:", StringComparison.Ordinal);
            if (rollback) fault.Check("marker-before");
            var result = base.SetName(link, name);
            if (rollback) fault.Check("marker-after");
            return result;
        }
    }

    public static TheoryData<string, string> FailureCases
    {
        get
        {
            var cases = new TheoryData<string, string>();
            foreach (var point in new[] { "undo-before", "undo-after", "data-before", "data-after",
                "marker-before", "marker-after", "log-before", "log-after" })
                foreach (var entry in new[] { "transaction", "owner", "graph" }) cases.Add(point, entry);
            return cases;
        }
    }

    [Theory]
    [MemberData(nameof(FailureCases))]
    public void FailedVersionRollbackRetainsAdmissionAndOwnerCleanupRetry(string point, string entry)
    {
        var directory = Directory.CreateTempSubdirectory("version-rollback-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        var branchPath = Path.Combine(directory, "branches.links");
        try
        {
            uint a, b; long head;
            var fault = new Fault();
            using (var data = new Store(dataPath, fault))
            using (var log = new Store(logPath, fault))
            using (var branches = new NamedTypesDecorator<uint>(branchPath))
            using (var tx = new TransactionsDecorator(data, log,
                flushData: () => { fault.Check("data-before"); data.Flush(); fault.Check("data-after"); },
                flushLog: () => { fault.Check("log-before"); log.Flush(); fault.Check("log-after"); }))
            using (var vc = new VersionControlDecorator(tx, branches))
            {
                a = vc.CreateAndUpdate(11u, 12u); b = vc.CreateAndUpdate(21u, 22u);
                head = vc.CurrentSequence;
                using var pending = vc.BeginTransaction();
                vc.Update(new[] { a }, new[] { a, 31u, 32u }, null);
                vc.Update(new[] { b }, new[] { b, 41u, 42u }, null);
                void Cleanup()
                {
                    if (entry == "transaction") pending.Rollback();
                    else if (entry == "owner") vc.Dispose();
                    else LinksFacadeDisposer.Dispose(vc);
                }
                fault.Point = point;
                Assert.Contains(point, Assert.Throws<IOException>(Cleanup).Message);
                Assert.False(pending.IsCommitted); Assert.False(pending.IsRolledBack);
                Assert.Throws<InvalidOperationException>(() => vc.Branch("blocked"));
                Assert.Throws<InvalidOperationException>(() => vc.SwitchBranch("main"));
                Assert.Throws<InvalidOperationException>(() => vc.Checkout(0));
                Assert.Throws<InvalidOperationException>(() => vc.Tag("blocked"));
                Assert.Throws<InvalidOperationException>(() => vc.BeginTransaction());
                Assert.Throws<InvalidOperationException>(() => pending.Commit());
                Cleanup();
                Assert.True(pending.IsRolledBack);
                // Only the explicit graph helper owns the stores. Complete the
                // other ownership paths before exclusively reopening mappings.
                if (entry != "graph") LinksFacadeDisposer.Dispose(vc);
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
            Assert.Equal(2u, recovered.Count()); Assert.Equal(head, recovered.CurrentSequence);
            Assert.Equal(11u, recovered.GetSource(a)); Assert.Equal(12u, recovered.GetTarget(a));
            Assert.Equal(21u, recovered.GetSource(b)); Assert.Equal(22u, recovered.GetTarget(b));
            using (var next = recovered.BeginTransaction())
            {
                recovered.CreateAndUpdate(51u, 52u); next.Commit();
            }
            recovered.Tag("following");
            Assert.True(recovered.TryGetTag("following", out var following));
            Assert.Equal(recovered.CurrentSequence, following); Assert.True(following > head);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
