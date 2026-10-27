using Platform.Data;
using Platform.Data.Doublets;
using static Foundation.Data.Doublets.Cli.Tests.VersionPublicationIntentFailureTests;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionPublicationRecoveryTests
{
    public static TheoryData<string> Cases
    {
        get
        {
            var cases = new TheoryData<string>();
            foreach (var phase in new[] { "transition", "head", "applied", "publication-flush", "intent-remove", "intent-delete", "cleanup-flush" })
                foreach (var edge in new[] { "before", "after" }) cases.Add(phase + "-" + edge);
            return cases;
        }
    }

    [Theory(Timeout = 30000)]
    [MemberData(nameof(Cases))]
    public async Task RestartRetriesEveryInterruptedRecoveryPhase(string point)
    {
        await FiniteExitChild.WithDirectory("version-recovery-fault-", async directory =>
        {
            var (id, original, attempted) = await VersionProcessExitTests.ExitChild(directory, "checkpoint", false);
            using (var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links")))
            using (var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links")))
            using (var branches = new Metadata(Path.Combine(directory, "branches.links"),
                new Fault { Point = point, Flushes = 1 }))
            using (var tx = new TransactionsDecorator(data, log))
            {
                var error = Assert.Throws<IOException>(() => new VersionControlDecorator(tx, branches, trace: true));
                Assert.Contains(point, error.Message);
                Assert.Equal(21u, data.GetSource(id));
            }
            long following = 0;
            for (var restart = 0; restart < 2; ++restart)
            {
                using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
                using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
                using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
                using var tx = new TransactionsDecorator(data, log);
                using var vc = new VersionControlDecorator(tx, branches);
                Assert.Equal(VersionExitProbe.Topic, vc.CurrentBranch);
                Assert.Equal(restart == 0 ? attempted : following, vc.CurrentSequence);
                Assert.Equal(restart == 0 ? 21u : 31u, vc.GetSource(id));
                if (restart == 0)
                {
                    vc.Checkout(original); Assert.Equal(11u, vc.GetSource(id));
                    vc.Checkout(attempted); Assert.Equal(21u, vc.GetSource(id));
                    vc.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                    following = vc.CurrentSequence;
                }
                var any = branches.Constants.Any;
                Assert.DoesNotContain(branches.All(new Link<uint>(any, any, any)), row =>
                    row is not null && branches.GetName(row[0])?.StartsWith("__vc:pending:", StringComparison.Ordinal) == true);
                data.Flush(); log.Flush(); branches.Flush();
            }
        });
    }
}
