// Original minimum control; the baseline driver supplies old source and helpers.
using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public partial class ReclamationTests
{
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
}
