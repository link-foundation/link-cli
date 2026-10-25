using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Tests;

public class AutomaticVersionWriteErrorTests
{
    [Fact]
    public void AutomaticAsyncWritesPublishAndJoinTheirCheckpointBeforeReopening()
    {
        var directory = Directory.CreateTempSubdirectory("version-automatic-async-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        var branchPath = Path.Combine(directory, "branches.links");
        try
        {
            uint id;
            long original, expected;
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var branches = new NamedTypesDecorator<uint>(branchPath))
            using (var tx = new TransactionsDecorator(data, log, commitMode: CommitMode.Async))
            using (var vc = new VersionControlDecorator(tx, branches))
            {
                id = vc.CreateAndUpdate(11u, 12u);
                original = vc.CurrentSequence;
                vc.Branch("topic"); vc.SwitchBranch("topic");
                vc.Update(new[] { id }, new[] { id, 21u, 22u }, null);
                expected = vc.CurrentSequence;
                vc.Tag("following");
                Assert.Equal(tx.LastLoggedSequence, expected);
                Assert.True(expected > original);
                // Shutdown is allowed to cancel queued checkpoints. First
                // observe ordinary asynchronous completion, then join it.
                Assert.True(SpinWait.SpinUntil(() => tx.AppliedSequence == expected, TimeSpan.FromSeconds(5)));
                tx.Shutdown();
                Assert.Equal(expected, tx.AppliedSequence);
                data.Flush(); log.Flush(); branches.Flush();
            }
            using var dataAgain = new NamedTypesDecorator<uint>(dataPath);
            using var logAgain = new NamedTypesDecorator<uint>(logPath);
            using var branchesAgain = new NamedTypesDecorator<uint>(branchPath);
            using var txAgain = new TransactionsDecorator(dataAgain, logAgain);
            using var reopened = new VersionControlDecorator(txAgain, branchesAgain);
            Assert.Equal(expected, reopened.CurrentSequence);
            Assert.Equal("topic", reopened.CurrentBranch);
            Assert.Equal(expected, reopened.ListTags()["following"]);
            Assert.Equal(21u, reopened.GetSource(id));
            reopened.Checkout(original);
            Assert.Equal(11u, reopened.GetSource(id));
            reopened.Checkout(expected);
            Assert.Equal(21u, reopened.GetSource(id));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    private sealed class Log(string path) : NamedTypesDecorator<uint>(path)
    {
        public string? Point;
        public bool FailRollback;
        public override uint SetName(uint link, string name)
        {
            if (FailRollback && name.StartsWith("__transactions:rollback:", StringComparison.Ordinal))
            {
                FailRollback = false;
                throw new IOException("injected undo publication");
            }
            var fail = name.StartsWith("__transactions:transition:", StringComparison.Ordinal) ? Point : null;
            if (fail is not null) Point = null;
            if (fail == "before") throw new IOException("injected transition before append");
            var result = base.SetName(link, name);
            if (fail == "after") throw new IOException("injected transition after append");
            return result;
        }
    }

    public static TheoryData<string, string, bool> Cases
    {
        get
        {
            var cases = new TheoryData<string, string, bool>();
            foreach (var operation in new[] { "create", "update", "delete" })
                foreach (var failure in new[] { "callback", "before", "after" })
                    foreach (var retry in new[] { false, true }) cases.Add(operation, failure, retry);
            return cases;
        }
    }

    [Theory]
    [MemberData(nameof(Cases))]
    public void PrecommitFailuresUndoWithoutPublishingAnAbortedVersion(string operation, string failure, bool retry)
    {
        var directory = Directory.CreateTempSubdirectory("version-write-error-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        var branchPath = Path.Combine(directory, "branches.links");
        try
        {
            uint id;
            long original, expected;
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new Log(logPath))
            using (var branches = new NamedTypesDecorator<uint>(branchPath))
            using (var tx = new TransactionsDecorator(data, log))
            using (var vc = new VersionControlDecorator(tx, branches))
            {
                id = vc.CreateAndUpdate(11u, 12u);
                original = vc.CurrentSequence;
                log.Point = failure == "callback" ? null : failure;
                log.FailRollback = retry;
                WriteHandler<uint>? handler = failure == "callback"
                    ? (_, _) => throw new IOException("injected callback") : null;
                var error = Assert.Throws<IOException>(() =>
                {
                    if (operation == "create") vc.Create(null, handler);
                    else if (operation == "delete") vc.Delete(new[] { id }, handler);
                    else vc.Update(new[] { id }, new[] { id, 21u, 22u }, handler);
                });
                Assert.Contains(retry ? "undo publication" : failure == "callback" ? "callback" : "transition", error.Message);
                if (retry)
                {
                    Assert.Throws<InvalidOperationException>(() => vc.Branch("blocked"));
                    Assert.Throws<InvalidOperationException>(() => vc.Create(null, null));
                    log.FailRollback = true;
                    Assert.Throws<IOException>(vc.Dispose);
                    Assert.Throws<InvalidOperationException>(() => vc.BeginTransaction());
                    vc.Dispose();
                }
                Assert.Equal(original, vc.CurrentSequence);
                Assert.Equal(1u, vc.Count());
                Assert.Equal(11u, vc.GetSource(id));
                Assert.Equal(12u, vc.GetTarget(id));
                var aborted = tx.Log.Where(t => t.Sequence > original).Select(t => t.Sequence).ToArray();
                Assert.NotEmpty(aborted);
                foreach (var sequence in aborted) Assert.Throws<InvalidOperationException>(() => vc.Checkout(sequence));
                vc.Branch("following");
                vc.SwitchBranch("following");
                vc.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                expected = vc.CurrentSequence;
                Assert.Equal(tx.LastLoggedSequence, expected);
                vc.Checkout(original);
                Assert.Equal(11u, vc.GetSource(id));
                vc.Checkout(expected);
                Assert.Equal(31u, vc.GetSource(id));
                data.Flush(); log.Flush(); branches.Flush();
            }
            using var recoveredData = new NamedTypesDecorator<uint>(dataPath);
            using var recoveredLog = new NamedTypesDecorator<uint>(logPath);
            using var recoveredBranches = new NamedTypesDecorator<uint>(branchPath);
            using var recoveredTx = new TransactionsDecorator(recoveredData, recoveredLog);
            using var recovered = new VersionControlDecorator(recoveredTx, recoveredBranches);
            Assert.Equal("following", recovered.CurrentBranch);
            Assert.Equal(expected, recovered.CurrentSequence);
            Assert.Equal(1u, recovered.Count());
            Assert.Equal(31u, recovered.GetSource(id));
            recovered.Checkout(original);
            Assert.Equal(11u, recovered.GetSource(id));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
