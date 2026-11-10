using System.Text.Json;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionRowUndoCodecTests
{
    private static JsonDocument Vectors() => JsonDocument.Parse(File.ReadAllText(Path.Combine(
        AppContext.BaseDirectory, "Protocol", "transaction-undo-vectors.json")));

    [Fact]
    public void CanonicalImagesShareFixturesAndOldRecordReadersRejectThem()
    {
        using var vectors = Vectors();
        foreach (var vector in vectors.RootElement.GetProperty("records").EnumerateArray())
        {
            var wire = vector.GetProperty("wire").GetString()!;
            Assert.Equal(wire, TransactionUndoImage<uint>.Parse(wire).Serialize());
            Assert.False(JournalRecord<uint>.TryParse(wire, out _));
        }
        var wide = vectors.RootElement.GetProperty("wide").GetString()!;
        Assert.Equal(wide, TransactionUndoImage<ulong>.Parse(wide).Serialize());
        Assert.Throws<InvalidDataException>(() => TransactionUndoImage<uint>.Parse(wide));
        // Preserve C#'s existing UTF-16 string domain; Rust rejects lone surrogates.
        var unpaired = vectors.RootElement.GetProperty("unpairedUtf16").GetString()!;
        Assert.Equal(unpaired, TransactionUndoImage<uint>.Parse(unpaired).Serialize());
    }

    public static TheoryData<string> InvalidImages
    {
        get
        {
            using var vectors = Vectors();
            return new(vectors.RootElement.GetProperty("invalid").EnumerateArray().Select(value => value.GetString()!));
        }
    }

    [Theory]
    [MemberData(nameof(InvalidImages))]
    public void InvalidImagePreservesRowsNamesHistoryAndRetryAdmission(string payload)
    {
        var directory = Directory.CreateTempSubdirectory("undo-validation-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
            using var tx = new TransactionsDecorator(data, log);
            var id = tx.CreateAndUpdate(11u, 12u); tx.SetName(id, "original");
            var history = tx.Journal.ToArray();
            var marker = log.CreateAndUpdate(0u, 0u); log.SetName(marker, "__transactions:transition:" + payload);
            for (var retry = 0; retry < 2; retry++)
            {
                Assert.Throws<InvalidDataException>(tx.Recover);
                Assert.Equal(history, tx.Journal); Assert.Equal("original", tx.GetName(id));
                Assert.Equal(11u, tx.GetSource(id)); Assert.Equal(12u, tx.GetTarget(id));
                Assert.Throws<InvalidOperationException>(() => tx.CreateAndUpdate(21u, 22u));
            }
            log.SetName(marker, "repaired"); tx.Recover();
            tx.SetName(id, "following"); tx.Recover(); Assert.Equal("following", tx.GetName(id));
        }
        finally { Directory.Delete(directory, true); }
    }
}
