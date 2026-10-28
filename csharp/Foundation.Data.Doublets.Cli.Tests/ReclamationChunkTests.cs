using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public partial class ReclamationTests
{
    private static JournalChunkOptions ChunkSelection => new(2, 64, 32);
    private static ReclaimingPartitionedJournalStore OpenChunks(ReclamationMemoryStore c, ReclamationMemoryStore a, ReclamationMemoryStore b, long cacheBytes = 0) =>
        ReclaimingPartitionedJournalStore.Open(c, new Dictionary<string, IJournalMaintenanceStore> { ["east"] = a, ["west"] = b }, key => key == "a" ? "east" : "west", new ReclamationOptions(CacheBytes: cacheBytes), ChunkSelection);
    [Fact(Timeout = 10000)]
    public async Task SelectedChunksReuseWithinEpochAndRestageAfterFence()
    {
        await Task.Yield();
        var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore();
        var store = ReclaimingPartitionedJournalStore.Open(c,
            new Dictionary<string, IJournalMaintenanceStore> { ["east"] = a }, _ => "east",
            new ReclamationOptions(CacheBytes: 256), new JournalChunkOptions(2, 64, 32));
        var keys = new[] { "a" }; var seed = new[] { "aa", "bb", "cc", "dd" };
        Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { seed }));
        var original = c.Values["a"]; Assert.Equal("chunks2", original.Entries[0]); Assert.Equal("2", original.Entries[3]);
        var extended = seed.Append("ee").ToArray();
        Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { extended }));
        Assert.Equal(original.Entries.Skip(4).Take(6), c.Values["a"].Entries.Skip(4).Take(6));
        Assert.Equal(3, a.Count("swap")); var owned = store.Read(keys);
        Assert.Equal(1UL, store.CollectGarbage()!.Epoch);
        var fresh = extended.Append("ff").ToArray();
        Assert.True(store.CompareExchange(keys, owned, new IReadOnlyList<string>?[] { fresh }));
        var root = c.Values["a"].Entries;
        for (var i = 4; i < root.Count; i += 3) Assert.StartsWith("blob2-0000000000000001-", root[i]);
        Assert.Equal(extended, owned[0].Entries);
        Assert.Equal(3, store.CollectGarbage()!.Deleted);
        Assert.Equal(fresh, store.Read(keys)[0].Entries);
    }
    [Fact(Timeout = 10000)]
    public async Task AllChunkOptionsAndUpdatesValidateBeforeAnyIo()
    {
        await Task.Yield();
        foreach (var chunks in new[] { new JournalChunkOptions(0), new JournalChunkOptions(MaxBytes: 8388609), new JournalChunkOptions(MaxChunks: 4097) })
        {
            var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore();
            Assert.Throws<ArgumentOutOfRangeException>(() => ReclaimingPartitionedJournalStore.Open(c, new Dictionary<string, IJournalMaintenanceStore> { ["east"] = a }, _ => "east", null, chunks));
            Assert.Empty(c.Calls); Assert.Empty(a.Calls);
        }
        var catalog = new ReclamationMemoryStore(); var data = new ReclamationMemoryStore();
        var store = ReclaimingPartitionedJournalStore.Open(catalog, new Dictionary<string, IJournalMaintenanceStore> { ["east"] = data }, _ => "east", null, new(1, 4, 2));
        foreach (var records in new[] { new[] { "12345" }, new[] { "a", "b", "c" }, new[] { "\ud800" } })
        {
            var calls = catalog.Calls.Count + data.Calls.Count;
            Assert.ThrowsAny<ArgumentException>(() => store.CompareExchange(Keys, new[] { new JournalVersion(0, Array.Empty<string>()), new JournalVersion(0, Array.Empty<string>()) }, new IReadOnlyList<string>?[] { new[] { "ok" }, records }));
            Assert.Equal(calls, catalog.Calls.Count + data.Calls.Count);
        }
    }
    [Fact(Timeout = 10000)]
    public async Task DuplicateChunksProtectEveryReferenceAndNamespaceAlias()
    {
        await Task.Yield(); var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore();
        var store = ReclaimingPartitionedJournalStore.Open(c, new Dictionary<string, IJournalMaintenanceStore> { ["east"] = a, ["west"] = a }, _ => "east", new ReclamationOptions(CacheBytes: 256), new(1, 64, 32));
        var keys = new[] { "a" }; var data = new[] { "same", "same" };
        Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { data }));
        Assert.Single(a.Values); Assert.Equal(0, store.CollectGarbage()!.Deleted); Assert.Equal(data, store.Read(keys)[0].Entries);
        var pointer = c.Values["a"].Entries.ToArray(); pointer[8] = "99"; c.Values["a"] = new(c.Values["a"].Generation, pointer);
        Assert.Throws<InvalidDataException>(() => store.CollectGarbage()); Assert.Equal(0, a.Count("delete"));
    }
}
