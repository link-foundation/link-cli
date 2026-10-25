using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionMovementMarkerTests
{
    [Fact]
    public void SharedMovementVectorsPreserveExactPathsAndPayloads()
    {
        using var vectors = JsonDocument.Parse(File.ReadAllText(Path.Combine(
            AppContext.BaseDirectory, "Protocol", "version-movement-vectors.json")));
        foreach (var row in vectors.RootElement.GetProperty("movements").EnumerateArray())
        {
            var marker = row.GetProperty("marker").GetString()!;
            Assert.True(VersionControlDecorator.TryDecodeMovement(marker, out var actual));
            Assert.Equal(row.GetProperty("fromBranch").GetString(), actual.FromBranch);
            Assert.Equal(row.GetProperty("toBranch").GetString(), actual.ToBranch);
            Assert.Equal(row.GetProperty("fromSequence").GetInt64(), actual.From);
            Assert.Equal(row.GetProperty("toSequence").GetInt64(), actual.To);
            Assert.Equal(row.GetProperty("undo").EnumerateArray().Select(r => r.GetString()), actual.Undo.Select(r => r.Serialize()));
            Assert.Equal(row.GetProperty("redo").EnumerateArray().Select(r => r.GetString()), actual.Redo.Select(r => r.Serialize()));
            Assert.Equal(marker, VersionControlDecorator.EncodeMovement(actual));
        }
        foreach (var row in vectors.RootElement.GetProperty("invalidMovements").EnumerateArray())
            Assert.False(VersionControlDecorator.TryDecodeMovement(row.GetString()!, out _), row.GetString());
        // .NET preserves unpaired code units, just as its branch/publication
        // markers do; Rust strings reject these two non-Unicode fixtures.
        foreach (var row in vectors.RootElement.GetProperty("nonUnicodeMovements").EnumerateArray())
        {
            var marker = row.GetString()!;
            Assert.True(VersionControlDecorator.TryDecodeMovement(marker, out var actual));
            Assert.Equal(marker, VersionControlDecorator.EncodeMovement(actual));
        }
    }
}
