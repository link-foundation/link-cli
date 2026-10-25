using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionNameCodecTests
{
    [Fact]
    public void SharedCompleteRecordsPreserveNamesAndRowCompatibility()
    {
        using var vectors = JsonDocument.Parse(File.ReadAllText(Path.Combine(
            AppContext.BaseDirectory, "Protocol", "transaction-name-vectors.json")));
        foreach (var row in vectors.RootElement.GetProperty("records").EnumerateArray())
        {
            var wire = row.GetProperty("wire").GetString()!;
            Assert.True(JournalRecord<uint>.TryParse(wire, out var actual), wire);
            Assert.Equal(wire, actual.Serialize());
            Assert.Equal(row.GetProperty("nameOnly").GetBoolean(), actual.NameOnly);
            var expected = row.GetProperty("names").EnumerateArray().ToArray();
            Assert.Equal(expected.Length, actual.Names.Count);
            foreach (var (name, wanted) in actual.Names.Zip(expected))
            {
                Assert.Equal(wanted.GetProperty("index").GetString(), name.Index.ToString());
                Assert.Equal(wanted.GetProperty("before").GetString(), name.Before);
                Assert.Equal(wanted.GetProperty("after").GetString(), name.After);
            }
        }
        foreach (var row in vectors.RootElement.GetProperty("invalid").EnumerateArray())
            Assert.False(JournalRecord<uint>.TryParse(row.GetString()!, out _), row.GetString());
        foreach (var row in vectors.RootElement.GetProperty("wide").EnumerateArray())
        {
            var wire = row.GetString()!;
            Assert.False(JournalRecord<uint>.TryParse(wire, out _));
            Assert.True(JournalRecord<ulong>.TryParse(wire, out var wide));
            Assert.Equal(wire, wide.Serialize());
        }
        // Preserve the existing .NET UTF-16 string domain; Rust rejects an
        // unpaired code unit before replay rather than replacing it silently.
        foreach (var row in vectors.RootElement.GetProperty("nonUnicode").EnumerateArray())
        {
            var wire = row.GetString()!;
            Assert.True(JournalRecord<uint>.TryParse(wire, out var actual));
            Assert.Equal(wire, actual.Serialize());
        }
        foreach (var row in vectors.RootElement.GetProperty("legacy").EnumerateArray())
        {
            Assert.True(JournalRecord<uint>.TryParse(row.GetString()!, out var actual));
            Assert.Equal(actual.Transition.Serialize(), actual.Serialize());
            Assert.False(actual.NameOnly); Assert.Empty(actual.Names);
        }
    }
}
