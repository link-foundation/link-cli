using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionPublicationMarkerTests
{
    private static JsonDocument Vectors() => JsonDocument.Parse(File.ReadAllText(
        Path.Combine(AppContext.BaseDirectory, "Protocol", "version-publication-vectors.json")));

    [Fact]
    public void SharedPublicationIntentsPreserveExactIdentityRangeAndBranch()
    {
        using var vectors = Vectors();
        foreach (var row in vectors.RootElement.GetProperty("intents").EnumerateArray())
        {
            var expected = new VersionControlDecorator.PublicationIntent(
                Guid.ParseExact(row.GetProperty("id").GetString()!, "N"),
                row.GetProperty("before").GetInt64(), row.GetProperty("after").GetInt64(),
                row.GetProperty("branch").GetString()!);
            var marker = row.GetProperty("marker").GetString()!;
            Assert.True(VersionControlDecorator.TryDecodePublicationMarker(marker, out var actual));
            Assert.Equal(expected, actual);
            Assert.Equal(marker, VersionControlDecorator.EncodePublicationMarker(expected));
        }
    }

    [Fact]
    public void SharedMalformedPublicationIntentsAreRejected()
    {
        using var vectors = Vectors();
        foreach (var row in vectors.RootElement.GetProperty("invalidIntents").EnumerateArray())
            Assert.False(VersionControlDecorator.TryDecodePublicationMarker(row.GetString()!, out _), row.GetString());
    }
}
