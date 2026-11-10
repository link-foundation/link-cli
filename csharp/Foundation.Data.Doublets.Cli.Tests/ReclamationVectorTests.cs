using Foundation.Data.Doublets.Cli.Engine;
using System.Globalization;
using System.Text.Json;
namespace Foundation.Data.Doublets.Cli.Tests;

public partial class ReclamationTests
{
    private static JsonDocument Vectors() => JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "reclamation-v1.json")));
    private static string[] Records(JsonElement value) => value.EnumerateArray().Select(item => item.GetString()!).ToArray();
    private static ReclamationMemoryStore Catalog(string epoch)
    {
        var c = new ReclamationMemoryStore { Revision = 1 }; c.Values[EpochKey] = new(1, new[] { "gc1", epoch }); return c;
    }
    [Fact(Timeout = 10000)]
    public async Task IndependentAddressesPreserveUtf8FramingAndAllEpochBits()
    {
        await Task.Yield(); using var vectors = Vectors(); var cases = vectors.RootElement.GetProperty("objects"); Assert.Equal(30, cases.GetArrayLength());
        foreach (var item in cases.EnumerateArray())
        {
            var c = Catalog(item.GetProperty("epoch").GetString()!); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore(); var store = Open(c, a, b);
            var keys = new[] { "a" }; var entries = Records(item.GetProperty("entries"));
            Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { entries }));
            Assert.Equal(new[] { item.GetProperty("key").GetString()! }, a.Values.Keys); Assert.Equal(entries, store.Read(keys)[0].Entries);
        }
    }
    [Fact(Timeout = 10000)]
    public async Task IndependentEpochsValidateBeforeAnyMutation()
    {
        await Task.Yield(); using var vectors = Vectors(); var cases = vectors.RootElement.GetProperty("epochs"); Assert.Equal(13, cases.GetArrayLength());
        foreach (var item in cases.EnumerateArray())
        {
            var c = Catalog("0"); c.Values[EpochKey] = new(1, Records(item.GetProperty("entries"))); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore();
            if (item.GetProperty("valid").GetBoolean()) Assert.NotNull(Open(c, a, b)); else Assert.Throws<InvalidDataException>(() => Open(c, a, b));
            Assert.Equal(0, c.Count("swap"));
        }
    }
    [Fact(Timeout = 10000)]
    public async Task IndependentPointersValidateBeforeAnyMutation()
    {
        await Task.Yield(); using var vectors = Vectors(); var cases = vectors.RootElement.GetProperty("pointers"); Assert.Equal(22, cases.GetArrayLength());
        foreach (var item in cases.EnumerateArray())
        {
            var c = Catalog("18446744073709551615"); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore(); var parts = Records(item.GetProperty("entries")); c.Values["a"] = new(1, parts);
            if (item.GetProperty("valid").GetBoolean())
            {
                var entries = Records(item.GetProperty("objectEntries")); var target = parts[1] == "east" ? a : b;
                target.Values[parts[2]] = new(ulong.Parse(parts[3], CultureInfo.InvariantCulture), entries);
                Assert.Equal(entries, Open(c, a, b).Read(new[] { "a" })[0].Entries);
            }
            else Assert.Throws<InvalidDataException>(() => Open(c, a, b));
            Assert.Equal(0, c.Count("swap"));
        }
    }
}
