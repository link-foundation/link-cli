using Platform.Data;
using Platform.Data.Doublets;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>What undo does when equal rows meet. The default store merges them
/// as they are written, so undo must bring a merged row back; a store that keeps
/// them must get each row back as recorded instead of merging it into its twin.</summary>
public sealed class TransactionDuplicateRestoreTests : IDisposable
{
    private readonly string _directory = Directory.CreateTempSubdirectory("tx-duplicates-").FullName;
    private readonly NamedTypesDecorator<uint> _data;
    private readonly NamedTypesDecorator<uint> _log;
    private readonly TransactionsDecorator _transactions;

    public TransactionDuplicateRestoreTests()
    {
        _data = new NamedTypesDecorator<uint>(Path.Combine(_directory, "data.links"));
        _log = new NamedTypesDecorator<uint>(Path.Combine(_directory, "log.links"));
        _transactions = new TransactionsDecorator(_data, _log, flushData: _data.Flush, flushLog: _log.Flush);
    }

    public void Dispose()
    {
        _transactions.Dispose();
        _log.Dispose();
        _data.Dispose();
        Directory.Delete(_directory, recursive: true);
    }

    [Fact]
    public void RollingBackAMergingUpdateBringsTheMergedRowBack()
    {
        var kept = _transactions.CreateAndUpdate(10u, 20u);
        var merged = _transactions.CreateAndUpdate(30u, 40u);
        using (var transaction = _transactions.BeginTransaction())
        {
            _transactions.Update(new[] { merged }, new[] { merged, 10u, 20u }, null);
            Assert.False(_data.Exists(merged));
            transaction.Rollback();
        }
        Assert.Equal(new DoubletLink(kept, 10u, 20u), new DoubletLink(_data.GetLink(kept)));
        Assert.True(_data.Exists(merged), "The merged row must come back.");
        Assert.Equal(new DoubletLink(merged, 30u, 40u), new DoubletLink(_data.GetLink(merged)));
    }
}
