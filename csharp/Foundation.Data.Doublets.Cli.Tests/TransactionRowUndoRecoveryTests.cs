using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionRowUndoRecoveryTests
{
    private const string Prefix = "__transactions:transition:";
    private static string Id(int owner) => owner.ToString("x32", System.Globalization.CultureInfo.InvariantCulture);
    private static string Image(int owner, long start, uint source, uint target) => $"{Prefix}v3|undo|{Id(owner)}|{start}|1,{source},{target}|";
    private static string Row(int owner, long sequence, uint beforeSource, uint beforeTarget, uint source, uint target)
        => $"{Prefix}v1|{Id(owner)}|{sequence}|0|1|1,{beforeSource},{beforeTarget}|1,{source},{target}";
    private static string Commit(int owner) => "__transactions:commit:" + Id(owner);
    private static void Verify(string[] entries, uint source, uint target)
    {
        var directory = Directory.CreateTempSubdirectory("undo-order-").FullName;
        try
        {
            using (var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data")))
            using (var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log")))
            {
                data.EnsureCreated(1u); data.Update(new[] { 1u }, new[] { 1u, 99u, 99u }, null);
                data.EnsureCreated(8u); data.Update(new[] { 8u }, new[] { 8u, 1u, 1u }, null);
                foreach (var entry in entries) log.SetName(log.CreateAndUpdate(0u, 0u), entry);
            }
            for (var reopen = 0; reopen < 2; reopen++)
            {
                using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
                using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
                using var tx = new TransactionsDecorator(data, log);
                var rows = tx.All(new[] { tx.Constants.Any, tx.Constants.Any, tx.Constants.Any }).ToArray();
                Assert.Single(rows); Assert.Equal(source, tx.GetSource(1u)); Assert.Equal(target, tx.GetTarget(1u));
                if (reopen == 0) tx.SetName(77, "following"); else Assert.Equal("following", tx.GetName(77));
            }
        }
        finally { Directory.Delete(directory, true); }
    }
    [Fact]
    public void ImageRestoreRedoesLaterAppliedCommits()
        => Verify([Image(1, 0, 11, 12), Row(2, 1, 11, 12, 31, 32), Commit(2), "__transactions:applied:1"], 31, 32);

    [Fact]
    public void OlderGapAndMixedPendingUndoKeepTheirOrder()
    {
        Verify([Row(7, 1, 11, 12, 21, 22), Commit(7), Image(1, 1, 21, 22), Row(9, 2, 21, 22, 99, 99),
            Row(3, 3, 21, 22, 31, 32), Commit(3), "__transactions:applied:3", Image(4, 3, 31, 32)], 31, 32);
        Verify([Row(9, 1, 11, 12, 21, 22), Image(1, 1, 21, 22)], 11, 12);
    }
    [Fact]
    public void DuplicateImagePositionCannotChangeRecoveryOrder()
        => Verify([Image(1, 0, 11, 12), Row(2, 1, 11, 12, 31, 32), Commit(2), Image(1, 0, 11, 12)], 31, 32);

    [Theory]
    [InlineData("conflict")]
    [InlineData("late")]
    [InlineData("ambiguous")]
    public void ContradictoryImagesFailBeforeRowsNamesOrHistoryChange(string kind)
    {
        var directory = Directory.CreateTempSubdirectory("undo-contradiction-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
            using var tx = new TransactionsDecorator(data, log);
            var id = tx.CreateAndUpdate(11u, 12u); tx.SetName(id, "original"); var history = tx.Journal.ToArray();
            foreach (var row in log.All(new[] { log.Constants.Any, log.Constants.Any, log.Constants.Any })) log.SetName(row![0], "old " + row[0]);
            var entries = kind switch
            {
                "conflict" => new[] { Image(1, 0, 11, 12), Image(1, 0, 21, 22) },
                "late" => new[] { Row(1, 1, 11, 12, 21, 22), Image(1, 1, 11, 12) },
                _ => new[] { Image(1, 0, 11, 12), Image(2, 0, 21, 22) },
            };
            foreach (var entry in entries) log.SetName(log.CreateAndUpdate(0u, 0u), entry);
            for (var retry = 0; retry < 2; retry++)
            {
                Assert.Throws<InvalidDataException>(tx.Recover); Assert.Equal(history, tx.Journal);
                Assert.Equal("original", tx.GetName(id)); Assert.Equal(11u, tx.GetSource(id)); Assert.Equal(12u, tx.GetTarget(id));
                Assert.Throws<InvalidOperationException>(() => tx.CreateAndUpdate(41u, 42u));
            }
        }
        finally { Directory.Delete(directory, true); }
    }
}
