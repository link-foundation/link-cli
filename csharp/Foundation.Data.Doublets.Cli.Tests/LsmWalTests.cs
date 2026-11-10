using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class LsmWalTests
{
    [Fact(Timeout = 30000)]
    public Task IndependentWalVectorsAndCheckpointRoundtrip() => Task.Run(() =>
    {
        using var json = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol/native-lsm-wal-v1.json")));
        foreach (var fixture in json.RootElement.GetProperty("cases").EnumerateArray())
        {
            int Option(string name, int fallback) => fixture.TryGetProperty("options", out var options) && options.TryGetProperty(name, out var value) ? value.GetInt32() : fallback;
            var selected = new LsmOptions(Option("maxKeys", 4096), Option("maxBytes", 8388608), Option("bufferKeys", 64), Option("bufferBytes", 1048576), Option("maxRuns", 4), Option("maxWalRecords", 1024), Option("maxWalBytes", 16777216));
            var elements = fixture.GetProperty("records").EnumerateArray().ToArray();
            // The C# log contract cannot supply non-string records.
            if (elements.Any(v => v.ValueKind != JsonValueKind.String)) continue;
            var records = elements.Select(v => v.GetString()!).ToArray();
            if (fixture.GetProperty("valid").GetBoolean())
            {
                var state = LsmWal.Recover(records, selected);
                string encoded = LsmWal.Encode(state, selected);
                // Equivalent JSON escaping is allowed; compare parsed shapes.
                using var actual = JsonDocument.Parse(encoded);
                Assert.Equal(JsonSerializer.Serialize(fixture.GetProperty("checkpoint")), JsonSerializer.Serialize(actual.RootElement));
                Assert.Equal(encoded, LsmWal.Encode(LsmWal.Recover(new[] { encoded }, selected), selected));
            }
            else Assert.NotNull(Record.Exception(() => LsmWal.Recover(records, selected)));
        }
    });
    [Fact(Timeout = 30000)]
    public Task EncodingRevalidatesPublicStateAndRecoveryOwnsRows() => Task.Run(() =>
    {
        var records = new[] { "[\"native-lsm1\",\"1\",\"0\",\"0\",[],[[\"a\",\"1\",[\"value\"]]]]" };
        var state = LsmWal.Recover(records);
        ((string[])state.Buffer[0].Entries!)[0] = "caller";
        Assert.Equal("value", LsmWal.Recover(records).Buffer[0].Entries![0]);
        Assert.NotNull(Record.Exception(() => LsmWal.Encode(state with { Generation = 0 })));
        Assert.Throws<ArgumentNullException>(() => LsmWal.Recover(null!));
        Assert.Throws<ArgumentNullException>(() => LsmWal.Encode(null!));
    });
}
