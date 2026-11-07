using Foundation.Data.Doublets.Cli.Engine;
using System.Globalization;
using System.Text.Json;
namespace Foundation.Data.Doublets.Cli.Tests;

public partial class ReclamationTests
{
    [Fact(Timeout = 10000)]
    public async Task IndependentChunkVectorsPreserveUtf8AndAllEpochBits()
    {
        await Task.Yield(); using var vectors = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "reclaiming-chunks-vectors.json")));
        Assert.Equal(18, vectors.RootElement.GetArrayLength());
        foreach (var item in vectors.RootElement.EnumerateArray())
        {
            var c = Catalog(item.GetProperty("epoch").GetString()!); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore();
            var store = ReclaimingPartitionedJournalStore.Open(c, new Dictionary<string, IJournalMaintenanceStore> { ["east"] = a }, _ => "east", null,
                new(item.GetProperty("maxRecords").GetInt32(), item.GetProperty("maxBytes").GetInt32(), 32));
            var keys = new[] { "a" }; var data = Records(item.GetProperty("entries"));
            Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { data }));
            var chunks = item.GetProperty("chunks"); var expected = new List<string> { "chunks2", "east", data.Length.ToString(CultureInfo.InvariantCulture), chunks.GetArrayLength().ToString(CultureInfo.InvariantCulture) }; var index = 0;
            foreach (var chunk in chunks.EnumerateArray()) expected.AddRange(new[] { chunk.GetProperty("address").GetString()!, (++index).ToString(CultureInfo.InvariantCulture), Records(chunk.GetProperty("entries")).Length.ToString(CultureInfo.InvariantCulture) });
            Assert.Equal(expected, c.Values["a"].Entries); Assert.Equal(data, store.Read(keys)[0].Entries);
            Assert.Equal(data, Open(c, a, b).Read(keys)[0].Entries);
        }
    }
    [Fact(Timeout = 10000)]
    public async Task MalformedAndFutureChunkMetadataPrecedesAllDataReadsAndDeletion()
    {
        await Task.Yield(); var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore(); var store = OpenChunks(c, a, b);
        var keys = new[] { "a" }; Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { new[] { "one" } }));
        var valid = c.Values["a"].Entries; var k = valid[4]; var g = valid[5];
        var cases = new[] {
            new[] { "chunks2" }, new[] { "chunks2", "missing", "0", "0" }, new[] { "chunks2", "east", "00", "0" },
            new[] { "chunks2", "east", "4294967296", "0" }, new[] { "chunks2", "east", "0", "4097" },
            new[] { "chunks2", "east", "0", "01" }, new[] { "chunks2", "east", "0", "1" }, new[] { "chunks2", "east", "1", "0" },
            new[] { "chunks2", "east", "1", "1", "../bad", g, "1" }, new[] { "chunks2", "east", "1", "1", k, "0", "1" },
            new[] { "chunks2", "east", "1", "1", k, "01", "1" }, new[] { "chunks2", "east", "1", "1", k, "18446744073709551616", "1" },
            new[] { "chunks2", "east", "1", "1", k, g, "0" }, new[] { "chunks2", "east", "1", "1", k, g, "65537" },
            new[] { "chunks2", "east", "1", "1", k, g, "-1" }, new[] { "chunks2", "east", "2", "1", k, g, "1" },
            new[] { "chunks2", "east", "1", "1", "blob2-ffffffffffffffff-0000000000000000", g, "1" }
        };
        foreach (var entries in cases)
        {
            c.Values["b"] = new(1, entries); var reads = a.Count("read");
            Assert.Throws<InvalidDataException>(() => store.Read(Keys)); Assert.Equal(reads, a.Count("read"));
            Assert.Throws<InvalidDataException>(() => store.CollectGarbage()); Assert.Equal(0, a.Count("delete") + b.Count("delete"));
            c.Values[EpochKey] = new(c.Values[EpochKey].Generation, new[] { "gc1", "0" });
        }
    }
}
