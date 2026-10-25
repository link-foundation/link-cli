using System.Text.Json;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionNameRecoveryTests
{
    private static string[] Vectors()
    {
        using var vectors = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "transaction-name-vectors.json")));
        var invalid = vectors.RootElement.GetProperty("invalid").EnumerateArray().Select(row => row.GetString()!).ToList();
        var first = vectors.RootElement.GetProperty("records")[0].GetProperty("wire").GetString()!;
        invalid.Add(first + "\n" + first.Replace("1,-,", "1,-,0078", StringComparison.Ordinal));
        return invalid.ToArray();
    }
    public static TheoryData<string> InvalidRecords => new(Vectors());

    [Theory]
    [MemberData(nameof(InvalidRecords))]
    public void InvalidOrConflictingNamesPreservePopulatedRowsNamesAndHistory(string records)
    {
        var directory = Directory.CreateTempSubdirectory("name-validation-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "journal"));
            using var tx = new TransactionsDecorator(data, log);
            var id = tx.CreateAndUpdate(11u, 12u);
            tx.SetName(id, "original");
            var history = tx.Journal.ToArray();
            foreach (var row in log.All(new Link<uint>(log.Constants.Any, log.Constants.Any, log.Constants.Any)).ToArray())
                log.SetName(row![0], "saved history " + row[0]);
            var markers = records.Split('\n').Select(record =>
            {
                var marker = log.CreateAndUpdate(0u, 0u);
                log.SetName(marker, "__transactions:transition:" + record); return marker;
            }).ToArray();
            for (var retry = 0; retry < 2; retry++)
            {
                Assert.Throws<InvalidDataException>(tx.Recover);
                Assert.Equal(history, tx.Journal);
                Assert.Equal("original", tx.GetName(id));
                Assert.Equal(11u, tx.GetSource(id)); Assert.Equal(12u, tx.GetTarget(id));
                Assert.Throws<InvalidOperationException>(() => tx.SetName(id, "blocked"));
            }
            foreach (var marker in markers) log.SetName(marker, "repaired " + marker);
            tx.Recover(); tx.SetName(id, "following"); tx.Recover();
            Assert.Equal("following", tx.GetName(id));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void RetentionKeepsCompleteCommittedNamesAndReopening(bool chunked)
    {
        var directory = Directory.CreateTempSubdirectory("name-retention-").FullName;
        var archive = Path.Combine(directory, "archive");
        try
        {
            uint id;
            using (var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data")))
            using (var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "journal")))
            using (var tx = new TransactionsDecorator(data, log, chunked ? new LogRetentionPolicy.Chunked(1, archive) : new LogRetentionPolicy.Sized(1)))
            {
                id = tx.CreateAndUpdate(11u, 12u);
                tx.SetName(id, "original"); tx.SetName(id, "changed 雪|\0\n");
                using (var pending = tx.BeginTransaction())
                { tx.SetName(77, "changed 雪|\0\n"); pending.Rollback(); }
                Assert.Equal("changed 雪|\0\n", tx.GetName(id));
                tx.Delete(new[] { id }, null); tx.SetName(77, "following");
            }
            for (var reopen = 0; reopen < 2; reopen++)
            {
                using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
                using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "journal"));
                using var tx = new TransactionsDecorator(data, log);
                Assert.False(tx.Exists(id)); Assert.Null(tx.GetName(id)); Assert.Equal("following", tx.GetName(77));
                tx.SetName(88, "next");
            }
            if (chunked)
            {
                var records = Directory.GetFiles(archive).SelectMany(File.ReadAllLines).Select(line =>
                {
                    Assert.True(JournalRecord<uint>.TryParse(line, out var record)); return record;
                }).OrderBy(record => record.Sequence).ToArray();
                Assert.Contains(records, record => record.NameOnly);
                Assert.Contains(records, record => record.Kind == TransitionKind.Delete && record.Names.Count != 0);
                using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "restored"));
                using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "restored-journal"));
                using var tx = new TransactionsDecorator(data, log);
                foreach (var record in records) tx.ApplyRecord(record);
                Assert.False(tx.Exists(id)); Assert.Equal("following", tx.GetName(77));
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
