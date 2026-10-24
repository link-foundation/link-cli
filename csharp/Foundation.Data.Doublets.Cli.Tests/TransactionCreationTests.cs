using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionCreationTests
{
    [Theory]
    [InlineData(false, false)]
    [InlineData(false, true)]
    [InlineData(true, false)]
    [InlineData(true, true)]
    public void ResolvedCreationsPreserveOriginalRows(bool distinct, bool recover)
    {
        var directory = Path.Combine(Path.GetTempPath(), "creation-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        uint a, b, c;
        try
        {
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var tx = new TransactionsDecorator(data, log))
            {
                a = tx.CreateAndUpdate(0u, 0u);
                b = tx.CreateAndUpdate(a, a);
                c = tx.CreateAndUpdate(b, b);
                // Keep the handle undisposed in the recovery case: disposal
                // would perform a normal rollback and conceal recovery bugs.
                var transaction = tx.BeginTransaction();
                tx.CreateAndUpdate(a, distinct ? c : a);
                Assert.Equal(0u, tx.GetSource(a));
                Assert.Equal(0u, tx.GetTarget(a));
                Assert.Equal(distinct ? 4u : 3u, tx.Count());
                if (recover)
                {
                    data.Flush();
                    log.Flush();
                }
                else
                {
                    transaction.Rollback();
                    AssertBaseline(tx, a, b, c);
                    transaction.Dispose();
                }
            }
            using var reopenedData = new NamedTypesDecorator<uint>(dataPath);
            using var reopenedLog = new NamedTypesDecorator<uint>(logPath);
            using var reopened = new TransactionsDecorator(reopenedData, reopenedLog);
            AssertBaseline(reopened, a, b, c);
            using var next = reopened.BeginTransaction();
            next.Rollback();
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    private static void AssertBaseline(TransactionsDecorator tx, uint a, uint b, uint c)
    {
        Assert.Equal(3u, tx.Count());
        Assert.Equal(0u, tx.GetSource(a)); Assert.Equal(0u, tx.GetTarget(a));
        Assert.Equal(a, tx.GetSource(b)); Assert.Equal(a, tx.GetTarget(b));
        Assert.Equal(b, tx.GetSource(c)); Assert.Equal(b, tx.GetTarget(c));
    }
}
