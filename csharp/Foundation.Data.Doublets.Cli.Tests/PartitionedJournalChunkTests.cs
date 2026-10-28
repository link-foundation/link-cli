using Foundation.Data.Doublets.Cli.Engine;
using System.Text;
using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Tests;

public partial class PartitionedJournalChunkTests
{
    private sealed class Store : IAtomicJournalStore
    {
        public readonly Dictionary<string, JournalVersion> Values = new();
        public int Reads, Writes;
        public long Bytes;
        private ulong _revision;
        public Action? Before, After;
        public bool SupportsDurableFlush => true;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys)
        {
            Reads++;
            return keys.Select(key => Values.TryGetValue(key, out var item) ? new(item.Generation, item.Entries.ToArray()) : new JournalVersion(0, Array.Empty<string>())).ToArray();
        }
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            Before?.Invoke();
            if (keys.Where((key, i) => (Values.GetValueOrDefault(key)?.Generation ?? 0) != expected[i].Generation).Any()) return false;
            var generation = ++_revision;
            for (var i = 0; i < keys.Count; i++) if (updates[i] is { } data)
            {
                Values[keys[i]] = new(generation, data.ToArray());
                Bytes += data.Sum(record => Encoding.UTF8.GetByteCount(record));
            }
            Writes++; After?.Invoke(); return true;
        }
    }
    private static (Store Catalog, Store Data, PartitionedJournalStore Store) Fixture(JournalChunkOptions? chunks = null, long cache = 0)
    {
        var catalog = new Store(); var data = new Store();
        return (catalog, data, new(catalog, new Dictionary<string, IAtomicJournalStore> { ["east"] = data }, _ => "east", cache, chunks ?? new(2, 8, 32)));
    }
    [Fact]
    public void SharedIndependentChunkVectorsPreserveExactUtf8BoundariesAndEmptyJournals()
    {
        using var vectors = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "chunked-journal-vectors.json")));
        foreach (var vector in vectors.RootElement.EnumerateArray())
        {
            var (catalog, data, store) = Fixture(new(vector.GetProperty("maxRecords").GetInt32(), vector.GetProperty("maxBytes").GetInt32(), 32));
            var keys = new[] { "a" };
            var entries = vector.GetProperty("entries").EnumerateArray().Select(item => item.GetString()!).ToArray();
            Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { entries }));
            var chunks = vector.GetProperty("chunks").EnumerateArray().ToArray();
            var manifest = new List<string> { "chunks1", "east", entries.Length.ToString(System.Globalization.CultureInfo.InvariantCulture), chunks.Length.ToString(System.Globalization.CultureInfo.InvariantCulture) };
            for (var i = 0; i < chunks.Length; i++) manifest.AddRange(new[] { chunks[i].GetProperty("address").GetString()!, (i + 1).ToString(System.Globalization.CultureInfo.InvariantCulture), chunks[i].GetProperty("entries").GetArrayLength().ToString(System.Globalization.CultureInfo.InvariantCulture) });
            Assert.Equal(manifest, catalog.Values["a"].Entries);
            Assert.Equal(chunks.Length, data.Values.Count);
            Assert.Equal(entries, store.Read(keys)[0].Entries);
            var legacy = new PartitionedJournalStore(catalog, new Dictionary<string, IAtomicJournalStore> { ["east"] = data }, _ => "east");
            Assert.Equal(entries, legacy.Read(keys)[0].Entries);
        }
    }

    [Fact]
    public void AppendReusesImmutablePrefixesAndOwnedCacheButStagingChecksActualContents()
    {
        var (catalog, data, store) = Fixture(cache: 256); var keys = new[] { "a" };
        var seed = new[] { "aa", "bb", "cc", "dd" };
        Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { seed }));
        var pointer = catalog.Values["a"]; var owned = store.Read(keys); var reads = data.Reads;
        store.Read(keys); Assert.Equal(reads, data.Reads);
        var bytes = data.Bytes; var writes = data.Writes;
        Assert.True(store.CompareExchange(keys, owned, new IReadOnlyList<string>?[] { seed.Append("ee").ToArray() }));
        Assert.Equal(2, data.Bytes - bytes); Assert.Equal(1, data.Writes - writes);
        Assert.Equal(seed, owned[0].Entries);
        var fresh = store.Read(keys); ((string[])fresh[0].Entries)[0] = "caller";
        Assert.Equal(seed.Append("ee"), store.Read(keys)[0].Entries);
        catalog.Values["a"] = pointer; Assert.Equal(seed, store.Read(keys)[0].Entries);
        var key = pointer.Entries[4]; data.Values[key] = data.Values[key] with { Entries = new[] { "collision" } };
        Assert.Throws<InvalidDataException>(() => store.CompareExchange(keys, owned, new IReadOnlyList<string>?[] { seed }));
    }
    [Fact]
    public void EveryPolicyAndUpdateIsAdmittedBeforeStorageAccess()
    {
        foreach (var limits in new[] { new JournalChunkOptions(0, 1, 1), new(65537, 1, 1), new(1, 0, 1), new(1, 8388609, 1), new(1, 1, 0), new(1, 1, 4097) })
            Assert.Throws<ArgumentOutOfRangeException>(() => Fixture(limits));
        var (catalog, data, store) = Fixture(new(1, 4, 2));
        foreach (var records in new[] { new[] { "12345" }, new[] { "a", "b", "c" }, new[] { "\ud800" } })
        {
            Assert.ThrowsAny<ArgumentException>(() => store.CompareExchange(new[] { "a", "b" }, new[] { new JournalVersion(0, Array.Empty<string>()), new(0, Array.Empty<string>()) }, new IReadOnlyList<string>?[] { new[] { "one" }, records }));
            Assert.Equal(0, catalog.Reads + catalog.Writes + data.Reads + data.Writes);
        }
        var selected = new PartitionedJournalStore(catalog, new Dictionary<string, IAtomicJournalStore> { ["east"] = data }, key => key == "a" ? "east" : "missing", 0, new(1, 4, 2));
        Assert.Throws<ArgumentException>(() => selected.CompareExchange(new[] { "a", "b" }, new[] { new JournalVersion(0, Array.Empty<string>()), new(0, Array.Empty<string>()) }, new IReadOnlyList<string>?[] { new[] { "one" }, new[] { "two" } }));
        Assert.Equal(0, data.Reads + data.Writes);
    }
    [Fact]
    public void EverySelectedManifestValidatesBeforeTheFirstDataFetch()
    {
        var (catalog, data, store) = Fixture(); var keys = new[] { "a" };
        Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { new[] { "one" } }));
        var pointer = catalog.Values["a"]; var key = pointer.Entries[4]; var gen = pointer.Entries[5];
        var malformed = new[] {
            new[] { "chunks1" }, new[] { "chunks1", "missing", "0", "0" }, new[] { "chunks1", "east", "00", "0" },
            new[] { "chunks1", "east", "4294967296", "0" }, new[] { "chunks1", "east", "0", "4097" },
            new[] { "chunks1", "east", "0", "01" }, new[] { "chunks1", "east", "0", "1" }, new[] { "chunks1", "east", "1", "0" },
            new[] { "chunks1", "east", "1", "1", "../bad", gen, "1" }, new[] { "chunks1", "east", "1", "1", key, "0", "1" },
            new[] { "chunks1", "east", "1", "1", key, "01", "1" }, new[] { "chunks1", "east", "1", "1", key, "18446744073709551616", "1" },
            new[] { "chunks1", "east", "1", "1", key, gen, "0" }, new[] { "chunks1", "east", "1", "1", key, gen, "65537" },
            new[] { "chunks1", "east", "2", "1", key, gen, "1" }
        };
        foreach (var entries in malformed)
        {
            catalog.Values["b"] = new(1, entries); var reads = data.Reads;
            Assert.Throws<InvalidDataException>(() => store.Read(new[] { "a", "b" })); Assert.Equal(reads, data.Reads);
        }
    }
    [Fact]
    public void FinitePublicationCutsExposeOnlyCompleteGroupsAndFollowingEdits()
    {
        for (var cut = 0; cut < 5; cut++) foreach (var after in new[] { false, true })
        {
            var (catalog, data, store) = Fixture(); var keys = new[] { "a", "b" };
            var old = new IReadOnlyList<string>?[] { new[] { "old" }, new[] { "old-b" } };
            Assert.True(store.CompareExchange(keys, store.Read(keys), old)); var baseline = store.Read(keys); var mutation = 0;
            void Fault() { if (mutation++ == cut) throw new IOException("Finite publication cut."); }
            if (after) { catalog.After = Fault; data.After = Fault; } else { catalog.Before = Fault; data.Before = Fault; }
            var updates = new IReadOnlyList<string>?[] { new[] { "a", "b", "c" }, new[] { "d", "e", "f" } };
            Assert.Throws<IOException>(() => store.CompareExchange(keys, baseline, updates));
            catalog.Before = catalog.After = data.Before = data.After = null;
            var fresh = store.Read(keys);
            Assert.Equal(after && cut == 4 ? updates[0] : old[0], fresh[0].Entries);
            Assert.Equal(after && cut == 4 ? updates[1] : old[1], fresh[1].Entries);
            Assert.True(store.CompareExchange(keys, fresh, new IReadOnlyList<string>?[] { new[] { "next" }, null }));
            Assert.False(store.CompareExchange(keys, baseline, new IReadOnlyList<string>?[] { new[] { "stale" }, null }));
        }
    }

}
