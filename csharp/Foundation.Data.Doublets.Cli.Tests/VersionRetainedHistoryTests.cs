using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionRetainedHistoryTests
{
    [Theory]
    [InlineData("checkout")]
    [InlineData("switch")]
    [InlineData("redo")]
    public void MissingChangedHistoryRejectsBeforeAnyMutation(string entry)
    {
        var directory = Directory.CreateTempSubdirectory("version-retained-").FullName;
        try
        {
            uint id;
            long following;
            string selected;
            using (var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links")))
            using (var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links")))
            using (var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links")))
            using (var tx = new TransactionsDecorator(data, log, new LogRetentionPolicy.Sized(1)))
            using (var vc = new VersionControlDecorator(tx, branches))
            {
                id = vc.CreateAndUpdate(11u, 12u);
                var original = vc.CurrentSequence;
                vc.Branch("empty", 0); vc.Branch("safe", original);
                vc.Update(new[] { id }, new[] { id, 21u, 22u }, null);
                var current = vc.CurrentSequence;
                Assert.Single(tx.Log);
                if (entry == "redo")
                {
                    vc.SwitchBranch("safe");
                    vc.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                }
                var head = vc.CurrentSequence;
                var branch = vc.CurrentBranch;
                var before = vc.ListBranches();
                var source = vc.GetSource(id);
                Action attempt = entry switch { "checkout" => () => vc.Checkout(0), "switch" => () => vc.SwitchBranch("empty"), _ => () => vc.SwitchBranch("main") };
                Assert.Throws<InvalidOperationException>(attempt);
                Assert.Equal(head, vc.CurrentSequence); Assert.Equal(branch, vc.CurrentBranch);
                Assert.Equal(before, vc.ListBranches()); Assert.Equal(source, vc.GetSource(id));
                if (entry != "redo")
                {
                    // Unavailable shared-prefix records need no redo or undo.
                    vc.SwitchBranch("safe"); Assert.Equal(original, vc.CurrentSequence); Assert.Equal(11u, vc.GetSource(id));
                    vc.SwitchBranch("main"); Assert.Equal(current, vc.CurrentSequence); Assert.Equal(21u, vc.GetSource(id));
                    vc.Checkout(current); // A no-op needs no retained transitions.
                    vc.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                    var later = vc.CurrentSequence;
                    Assert.Throws<InvalidOperationException>(() => vc.Checkout(original));
                    Assert.Equal(later, vc.CurrentSequence); Assert.Equal(31u, vc.GetSource(id));
                }
                following = vc.CurrentSequence; selected = vc.CurrentBranch;
                data.Flush(); log.Flush(); branches.Flush();
            }
            using var reopenedData = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var reopenedLog = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            using var reopenedBranches = new NamedTypesDecorator<uint>(Path.Combine(directory, "branches.links"));
            using var reopenedTx = new TransactionsDecorator(reopenedData, reopenedLog);
            using var reopened = new VersionControlDecorator(reopenedTx, reopenedBranches);
            Assert.Equal(selected, reopened.CurrentBranch); Assert.Equal(following, reopened.CurrentSequence);
            Assert.Equal(31u, reopened.GetSource(id));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
