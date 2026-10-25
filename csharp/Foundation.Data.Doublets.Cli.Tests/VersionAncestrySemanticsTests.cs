using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionAncestrySemanticsTests
{
    [Fact(Timeout = 30000)]
    public async Task NestedAncestryPreservesCutoffsOrderAndFollowingWrites()
    {
        await Task.CompletedTask;
        var directory = Directory.CreateTempSubdirectory("ancestry-semantics-").FullName;
        try
        {
            uint id = 0;
            void Open(Action<VersionControlDecorator, TransactionsDecorator> action, bool seed = false)
            {
                using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
                using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
                using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
                if (seed) id = data.CreateAndUpdate(11u, 12u);
                using var tx = new TransactionsDecorator(data, log);
                using var vc = new VersionControlDecorator(tx, branches);
                action(vc, tx);
                data.Flush(); log.Flush(); branches.Flush();
            }
            Open((vc, _) =>
            {
                void Update(uint source) => vc.Update(new[] { id }, new[] { id, source, source + 1 }, null);
                Update(21); Update(31);
                vc.Branch("middle"); vc.SwitchBranch("middle");
                Update(41); Update(51);
                vc.Branch("leaf", 3); vc.SwitchBranch("leaf");
                Assert.Equal(41u, vc.GetSource(id));
                Update(61); Update(71);
                vc.SwitchBranch("middle");
                Assert.Equal(51u, vc.GetSource(id));
                Update(81);
                vc.SwitchBranch("main");
                Assert.Equal(31u, vc.GetSource(id));
                Update(91);
                vc.Branch("unrelated"); vc.SwitchBranch("unrelated"); Update(101);
                vc.SwitchBranch("leaf");
                foreach (var (sequence, source) in new[] { (6L, 71u), (5, 61u), (3, 41u), (2, 31u), (1, 21u), (0, 11u), (6, 71u) })
                {
                    vc.Checkout(sequence);
                    Assert.Equal(sequence, vc.CurrentSequence);
                    Assert.Equal(source, vc.GetSource(id));
                }
                var heads = vc.ListBranches();
                foreach (var missing in new[] { 4, 7, 8, 9 })
                {
                    Assert.Throws<InvalidOperationException>(() => vc.Checkout(missing));
                    Assert.Equal(6, vc.CurrentSequence);
                    Assert.Equal(71u, vc.GetSource(id));
                    Assert.Equal(heads, vc.ListBranches());
                }
                vc.Branch("cut", 5); vc.SwitchBranch("cut");
                Assert.Equal(61u, vc.GetSource(id));
                Assert.Throws<InvalidOperationException>(() => vc.Checkout(6));
                vc.Branch("empty", 0); vc.SwitchBranch("empty");
                Assert.Equal(11u, vc.GetSource(id));
                Update(111);
                Assert.Equal(10, vc.CurrentSequence);
            }, seed: true);
            for (var reopen = 0; reopen < 2; ++reopen)
                Open((vc, tx) =>
                {
                    Assert.Equal("empty", vc.CurrentBranch);
                    Assert.Equal(10, vc.CurrentSequence);
                    Assert.Equal(10, tx.LastLoggedSequence);
                    Assert.Equal(111u, vc.GetSource(id));
                    foreach (var (name, head) in new[] { ("main", 8L), ("middle", 7), ("leaf", 6), ("unrelated", 9), ("cut", 5), ("empty", 10) })
                        Assert.Equal(head, vc.ListBranches().Single(b => b.Name == name).Head);
                    vc.SwitchBranch("leaf"); vc.Checkout(3);
                    Assert.Equal(41u, vc.GetSource(id));
                    vc.SwitchBranch("empty");
                });
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
