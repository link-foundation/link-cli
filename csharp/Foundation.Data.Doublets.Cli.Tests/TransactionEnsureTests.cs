using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionEnsureTests
{
    [Fact]
    public void ExplicitAddressRollsBackAndCommittedAllocationReopens()
    {
        var directory = Path.Combine(Path.GetTempPath(), "ensure-transaction-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        var dataPath = Path.Combine(directory, "data"); var logPath = Path.Combine(directory, "log");
        try
        {
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var tx = new TransactionsDecorator(data, log))
            {
                using (var transaction = tx.BeginTransaction())
                {
                    tx.EnsureCreated(8u);
                    Assert.True(tx.Exists(8u));
                    transaction.Rollback();
                }
                Assert.Equal(0u, tx.Count());
                tx.EnsureCreated(8u);
                Assert.True(tx.Exists(8u));
                Assert.NotEmpty(tx.Log);
                Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
                var sequence = tx.LastLoggedSequence;
                tx.EnsureCreated(8u);
                Assert.Equal(sequence, tx.LastLoggedSequence);
                using var next = tx.BeginTransaction();
                next.Rollback();
            }
            using var reopenedData = new NamedTypesDecorator<uint>(dataPath);
            using var reopenedLog = new NamedTypesDecorator<uint>(logPath);
            using var reopened = new TransactionsDecorator(reopenedData, reopenedLog);
            Assert.True(reopened.Exists(8u));
            Assert.Equal(1u, reopened.Count());
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact]
    public void ExplicitAddressBelongsToVersionBranchAndSupportsCheckout()
    {
        var directory = Path.Combine(Path.GetTempPath(), "ensure-version-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
            using var branches = new NamedTypesDecorator<uint>(Path.Combine(directory, "versions"));
            using var tx = new TransactionsDecorator(data, log);
            using var vc = new VersionControlDecorator(tx, branches);
            vc.Branch("feature"); vc.SwitchBranch("feature");
            vc.EnsureCreated(8u);
            Assert.True(vc.CurrentSequence > 0);
            var head = vc.CurrentSequence;
            vc.SwitchBranch(VersionControlDecorator.DefaultBranchName);
            Assert.False(vc.Exists(8u));
            vc.SwitchBranch("feature");
            Assert.True(vc.Exists(8u));
            vc.Checkout(0);
            Assert.False(vc.Exists(8u));
            vc.Checkout(head);
            Assert.True(vc.Exists(8u));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
