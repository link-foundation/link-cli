using Platform.Data;
using Platform.Data.Doublets;
using static Foundation.Data.Doublets.Cli.Tests.VersionPublicationIntentFailureTests;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionMovementRestartFailureTests
{
    public static TheoryData<string> Cases
    {
        get
        {
            var cases = new TheoryData<string>();
            foreach (var phase in new[] { "current", "applied", "publication-flush", "intent-remove", "intent-delete", "cleanup-flush" })
                foreach (var edge in new[] { "before", "after" }) cases.Add(phase + "-" + edge);
            return cases;
        }
    }

    [Theory(Timeout = 30000)]
    [MemberData(nameof(Cases))]
    public async Task FailedConstructorKeepsRecoveryOwnerReachable(string point)
    {
        var directory = Directory.CreateTempSubdirectory("movement-restart-failure-").FullName;
        try
        {
            var (id, original, attempted) = await VersionProcessExitTests.ExitChild(directory, "intent-flush-after", true, "movement-exit");
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            var fault = new Fault { Point = point, Flushes = 1 };
            using var branches = new Metadata(Path.Combine(directory, "branches.links"), fault);
            using var tx = new TransactionsDecorator(data, log);
            try
            {
                var error = Assert.Throws<IOException>(() => new VersionControlDecorator(tx, branches));
                Assert.Contains(point, error.Message);
                Assert.Throws<InvalidOperationException>(() => tx.CreateAndUpdate(71u, 72u));
                Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
                Assert.Throws<InvalidOperationException>(() => tx.Shutdown());
                fault.Point = "flush-before";
                Assert.Throws<IOException>(() => new VersionControlDecorator(tx, branches));
                Assert.Throws<InvalidOperationException>(() => tx.CreateAndUpdate(71u, 72u));
                using var recovered = new VersionControlDecorator(tx, branches);
                Assert.Equal(VersionMovementExitTests.Topic, recovered.CurrentBranch);
                Assert.Equal(original, recovered.CurrentSequence);
                Assert.Equal(11u, recovered.GetSource(id));
                Assert.Equal(attempted, tx.LastLoggedSequence);
                recovered.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                Assert.Equal(attempted + 1, recovered.CurrentSequence);
                data.Flush(); log.Flush(); branches.Flush();
            }
            finally
            {
                // Release fixture resources even when the pre-fix orphaned
                // lease makes the assertions fail. No following writes occur.
                tx.FinishVersionReplay();
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
