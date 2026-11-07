using Platform.Data.Doublets;
using Fault = Foundation.Data.Doublets.Cli.Tests.VersionPublicationIntentFailureTests.Fault;
using Metadata = Foundation.Data.Doublets.Cli.Tests.VersionPublicationIntentFailureTests.Metadata;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionMovementFailureTests
{
    public static TheoryData<bool, string> Phases
    {
        get
        {
            var cases = new TheoryData<bool, string>();
            foreach (var phase in new[] { "intent-write", "intent-flush", "current", "publication-flush", "intent-remove", "intent-delete", "cleanup-flush" })
                foreach (var branchSwitch in new[] { false, true })
                    foreach (var edge in new[] { "before", "after" }) cases.Add(branchSwitch, phase + "-" + edge);
            return cases;
        }
    }

    [Theory(Timeout = 30000)]
    [MemberData(nameof(Phases))]
    [InlineData(false, "applied-before")]
    [InlineData(false, "applied-after")]
    [InlineData(false, "flush-before")]
    [InlineData(true, "applied-before")]
    [InlineData(true, "applied-after")]
    [InlineData(true, "flush-before")]
    public async Task MovementRetainsAdmissionUntilDurable(bool branchSwitch, string point)
    {
        await Task.CompletedTask;
        var directory = Directory.CreateTempSubdirectory("version-movement-").FullName;
        try
        {
            uint id;
            long following;
            var selected = branchSwitch ? "original" : "main";
            using (var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links")))
            using (var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links")))
            {
                var fault = new Fault();
                using var branches = new Metadata(Path.Combine(directory, "branches.links"), fault);
                using var tx = new TransactionsDecorator(data, log);
                using var vc = new VersionControlDecorator(tx, branches, trace: true);
                id = vc.CreateAndUpdate(11u, 12u);
                vc.SetName(id, "original 雪|\0\n");
                var original = vc.CurrentSequence;
                vc.Branch("original");
                vc.Update(new[] { id }, new[] { id, 21u, 22u }, null);
                vc.SetName(id, "changed 雪|\0\n");
                var attempted = vc.CurrentSequence;
                var transition = tx.Log.Last();
                var heads = vc.ListBranches();
                fault.Flushes = 0; fault.Point = point;
                Assert.Contains(point, Assert.Throws<IOException>(() =>
                {
                    if (branchSwitch) vc.SwitchBranch("original");
                    else vc.Checkout(original);
                }).Message);
                Assert.Null(fault.Point);
                void Fences()
                {
                    Assert.Throws<InvalidOperationException>(() => vc.Update(new[] { id }, new[] { id, 51u, 52u }, null));
                    Assert.Throws<InvalidOperationException>(() => vc.CreateAndUpdate(51u, 52u));
                    Assert.Throws<InvalidOperationException>(() => vc.Delete(new[] { id }, null));
                    Assert.Throws<InvalidOperationException>(() => vc.SetName(id, "blocked"));
                    Assert.Throws<InvalidOperationException>(() => vc.RemoveName(id));
                    Assert.Throws<InvalidOperationException>(() => vc.BeginTransaction());
                    Assert.Throws<InvalidOperationException>(() => vc.Branch("blocked"));
                    Assert.Throws<InvalidOperationException>(() => vc.Tag("blocked"));
                    Assert.Throws<InvalidOperationException>(() => vc.Checkout(0));
                    Assert.Throws<InvalidOperationException>(() => vc.SwitchBranch("main"));
                    Assert.Throws<InvalidOperationException>(vc.Recover);
                    Assert.Throws<InvalidOperationException>(() => tx.CreateAndUpdate(51u, 52u));
                    Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
                    Assert.Throws<InvalidOperationException>(tx.Recover);
                    Assert.Throws<InvalidOperationException>(() => tx.ApplyTransition(transition));
                    Assert.Throws<InvalidOperationException>(() => tx.RevertTransition(transition));
                    Assert.Equal(attempted, tx.LastLoggedSequence);
                }
                Fences();
                fault.Point = "flush-before";
                Assert.Contains("flush-before", Assert.Throws<IOException>(vc.Dispose).Message);
                Fences();
                vc.Dispose();
                Assert.Null(fault.Point);
                Assert.Equal(selected, vc.CurrentBranch);
                Assert.Equal(original, vc.CurrentSequence);
                Assert.Equal(heads, vc.ListBranches());
                Assert.Equal(11u, vc.GetSource(id));
                Assert.Equal("original 雪|\0\n", vc.GetName(id));
                vc.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                following = vc.CurrentSequence;
                Assert.Equal(attempted + 1, following);
                data.Flush(); log.Flush(); branches.Flush();
            }
            for (var reopen = 0; reopen < 2; ++reopen)
            {
                using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
                using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
                using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
                using var tx = new TransactionsDecorator(data, log);
                using var vc = new VersionControlDecorator(tx, branches);
                Assert.Equal(selected, vc.CurrentBranch);
                Assert.Equal(following, vc.CurrentSequence);
                Assert.Equal(31u, vc.GetSource(id));
                Assert.Equal("original 雪|\0\n", vc.GetName(id));
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
