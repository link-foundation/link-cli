using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public partial class PartitionedJournalChunkTests
{
    [Fact]
    public void MissingChangedCountUnicodeAndOversizedChunksFailClosed()
    {
        foreach (var kind in new[] { "missing", "generation", "contents", "count", "unicode", "bytes", "absent-records" })
        {
            var catalog = new Store(); var data = new Store();
            var store = new PartitionedJournalStore(catalog, new Dictionary<string, IAtomicJournalStore> { ["east"] = data }, _ => "east");
            var records = new[] { kind == "unicode" ? "\ud800" : kind == "bytes" ? new string('x', 8388609) : "one" };
            var keys = new[] { "a" }; Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { records }));
            var pointer = catalog.Values["a"]; var key = pointer.Entries[2]; var count = kind == "count" ? "2" : "1";
            catalog.Values["a"] = new(pointer.Generation, new[] { "chunks1", "east", count, "1", key, pointer.Entries[3], count });
            if (kind == "missing") data.Values.Remove(key);
            if (kind == "generation") data.Values[key] = data.Values[key] with { Generation = 99 };
            if (kind == "contents") data.Values[key] = data.Values[key] with { Entries = new[] { "other" } };
            if (kind == "absent-records") data.Values[key] = data.Values[key] with { Generation = 0 };
            Assert.Throws<InvalidDataException>(() => store.Read(keys));
        }
    }
    [Fact]
    public void CachedObjectsStillValidateEachManifestCount()
    {
        var (catalog, data, store) = Fixture(cache: 256); var keys = new[] { "a" };
        Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { new[] { "one" } }));
        store.Read(keys); var reads = data.Reads;
        var pointer = catalog.Values["a"]; var parts = pointer.Entries.ToArray(); parts[2] = parts[6] = "2";
        catalog.Values["a"] = pointer with { Entries = parts };
        Assert.Throws<InvalidDataException>(() => store.Read(keys)); Assert.Equal(reads, data.Reads);
    }
}
