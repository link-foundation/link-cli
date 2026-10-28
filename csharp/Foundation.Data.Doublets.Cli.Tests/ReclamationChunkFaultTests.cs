using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public partial class ReclamationTests
{
    [Fact(Timeout = 10000)]
    public async Task RetiredSuffixDiscardsCachedOrFetchedPrefixesAndOtherShards()
    {
        await Task.Yield();
        foreach (var cache in new[] { 0, 256 })
        {
            var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore(); var writer = OpenChunks(c, a, b); var reader = OpenChunks(c, a, b, cache);
            var old = new IReadOnlyList<string>?[] { new[] { "a0", "a0-tail", "a0-last" }, new[] { "b0", "b0-tail", "b0-last" } };
            Assert.True(writer.CompareExchange(Keys, writer.Read(Keys), old)); if (cache > 0) reader.Read(new[] { "a" });
            var before = writer.Read(Keys); var fresh = new IReadOnlyList<string>?[] { new[] { "a1", "a1-tail", "a1-last" }, new[] { "b1", "b1-tail", "b1-last" } };
            b.On("read", () => { }); b.On("read", () => { Assert.True(writer.CompareExchange(Keys, before, fresh)); Assert.Equal(4, writer.CollectGarbage()!.Deleted); });
            var group = reader.Read(Keys); for (var i = 0; i < 2; i++) Assert.Equal(fresh[i], group[i].Entries);
        }
    }
    [Fact(Timeout = 10000)]
    public async Task EveryChunkStagingAndPublicationCutPreservesCompleteGroups()
    {
        await Task.Yield();
        for (var cut = 0; cut < 5; cut++) foreach (var after in new[] { false, true })
        {
            var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore(); var store = OpenChunks(c, a, b); var old = Write(store, "old-a", "old-b");
            var updates = new IReadOnlyList<string>?[] { new[] { "a", "b", "c" }, new[] { "d", "e", "f" } };
            var provider = cut < 2 ? a : cut < 4 ? b : c;
            if (cut is 1 or 3) provider.On("swap", () => { }, after);
            provider.On("swap", () => throw new IOException("finite chunk cut"), after);
            Assert.Throws<IOException>(() => store.CompareExchange(Keys, old, updates));
            var fresh = store.Read(Keys); for (var i = 0; i < 2; i++) Assert.Equal(after && cut == 4 ? updates[i] : old[i].Entries, fresh[i].Entries);
            Assert.True(store.CompareExchange(Keys, fresh, new IReadOnlyList<string>?[] { new[] { "following" }, null }));
            Assert.False(store.CompareExchange(Keys, old, new IReadOnlyList<string>?[] { new[] { "stale" }, null }));
        }
    }
    [Fact(Timeout = 10000)]
    public async Task FenceAfterFinalChunkStagingConflictsWithoutPublicationReplay()
    {
        await Task.Yield(); var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore(); var writer = OpenChunks(c, a, b); var collector = OpenChunks(c, a, b);
        var keys = new[] { "a" }; var before = writer.Read(keys); var swaps = c.Count("swap");
        c.On("swap", () => Assert.Equal(1UL, collector.CollectGarbage()!.Epoch));
        Assert.False(writer.CompareExchange(keys, before, new IReadOnlyList<string>?[] { new[] { "a", "b", "c" } })); Assert.Equal(2, c.Count("swap") - swaps);
        Assert.Equal(before[0].Entries, writer.Read(keys)[0].Entries);
        Assert.True(writer.CompareExchange(keys, before, new IReadOnlyList<string>?[] { new[] { "following" } }));
    }
    [Fact(Timeout = 10000)]
    public async Task CachedChunkCountsActualGenerationsAndFreshStagingAreVerified()
    {
        await Task.Yield();
        foreach (var kind in new[] { "count", "cached-count", "generation", "contents", "missing", "staging" })
        {
            var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore(); var store = OpenChunks(c, a, b, kind is "cached-count" or "staging" ? 256 : 0);
            var keys = new[] { "a" }; Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { new[] { "one" } }));
            var owned = store.Read(keys); var pointer = c.Values["a"].Entries.ToArray(); var key = pointer[4];
            if (kind.Contains("count", StringComparison.Ordinal)) { pointer[2] = pointer[6] = "2"; c.Values["a"] = new(c.Values["a"].Generation, pointer); }
            if (kind == "generation") a.Values[key] = new(99, a.Values[key].Entries);
            if (kind is "contents" or "staging") a.Values[key] = new(a.Values[key].Generation, new[] { "damaged" });
            if (kind == "missing") a.Values.Remove(key);
            var reads = a.Count("read");
            if (kind == "staging") Assert.Throws<InvalidDataException>(() => store.CompareExchange(keys, owned, new IReadOnlyList<string>?[] { new[] { "one" } }));
            else Assert.Throws<InvalidDataException>(() => store.Read(keys));
            if (kind == "cached-count") Assert.Equal(reads, a.Count("read"));
        }
    }
}
