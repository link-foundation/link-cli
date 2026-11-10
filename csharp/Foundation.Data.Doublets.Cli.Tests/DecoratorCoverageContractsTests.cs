using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>Direct contracts of the undo log and transaction options.</summary>
public sealed class DecoratorCoverageContractsTests
{
    private static void WithStore(Action<NamedTypesDecorator<uint>, NamedTypesDecorator<uint>> action)
    {
        var directory = Directory.CreateTempSubdirectory("decorator-contracts-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            action(data, log);
        }
        finally { Directory.Delete(directory, true); }
    }

    [Theory]
    [InlineData("sized:-1", "Invalid sized")]
    [InlineData("sized:many", "Invalid sized")]
    [InlineData("chunked:", "Invalid chunked retention")]
    [InlineData("chunked::dir", "Invalid chunked retention")]
    [InlineData("chunked:10:", "Invalid chunked retention")]
    [InlineData("chunked:0:dir", "Invalid chunked size")]
    [InlineData("chunked:big:dir", "Invalid chunked size")]
    [InlineData("forever", "Unknown retention")]
    public void RetentionParsingRefusesMalformedSpecs(string spec, string message) =>
        Assert.Contains(message, Assert.Throws<ArgumentException>(() => LogRetentionPolicy.Parse(spec)).Message);

    [Fact]
    public void TransactionOptionsCanBeReadAndChangedLater() => WithStore((data, log) =>
    {
        using var tx = new TransactionsDecorator(data, log, flushData: data.Flush, flushLog: log.Flush);
        tx.CommitMode = CommitMode.Async;
        Assert.Equal(CommitMode.Async, tx.CommitMode);
        tx.CommitMode = CommitMode.Sync;
        tx.RetentionPolicy = new LogRetentionPolicy.Sized(9);
        Assert.Equal(new LogRetentionPolicy.Sized(9), tx.RetentionPolicy);
        Assert.Equal(((INamedTypesLinks<uint>)data).ReassignsNames, tx.ReassignsNames);
    });

    [Fact]
    public async Task AsyncBeginHonorsCancellationBeforeOpeningATransaction()
    {
        var directory = Directory.CreateTempSubdirectory("decorator-contracts-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            using var tx = new TransactionsDecorator(data, log, flushData: data.Flush, flushLog: log.Flush);
            using var cancelled = new CancellationTokenSource();
            await cancelled.CancelAsync();
            await Assert.ThrowsAsync<OperationCanceledException>(() => tx.BeginTransactionAsync(cancelled.Token));
            using var opened = await tx.BeginTransactionAsync();
            Assert.NotNull(opened);
            opened.Commit();
        }
        finally { Directory.Delete(directory, true); }
    }

    [Fact]
    public void UndoLogIgnoresUnchangedRowsAndAbsentNameHolders() => WithStore((links, _) =>
    {
        var log = new LinksUndoLog<uint>();
        Assert.True(log.IsEmpty);
        log.Record(new Link<uint>(1, 2, 3), new Link<uint>(1, 2, 3));
        log.KeepHolderOf(links, "nobody");
        Assert.True(log.IsEmpty);
        Assert.Empty(log.Changes);
    });

    [Fact]
    public void UndoRestoresADeletedRowWithItsPartsAndItsPreviousNames() => WithStore((links, _) =>
    {
        var row = links.CreateAndUpdate(11u, 12u);
        var renamed = links.CreateAndUpdate(13u, 14u);
        links.SetName(renamed, "held");
        var unnamed = links.CreateAndUpdate(15u, 16u);

        var log = new LinksUndoLog<uint>();
        log.KeepHolderOf(links, "held");
        log.KeepName(links, unnamed);
        Assert.False(log.IsEmpty);
        links.SetName(unnamed, "written");
        links.RemoveName(renamed);

        var before = new Link<uint>(links.GetLink(row));
        log.Record(before, null);
        links.Delete(row);
        Assert.False(links.Exists(row));

        log.Undo(links);

        Assert.Equal(before, new Link<uint>(links.GetLink(row)));
        Assert.Equal("held", links.GetName(renamed));
        Assert.Null(links.GetName(unnamed));
    });
}
