using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public partial class ReclamationTests
{
    private static void PublishChunks(ReclaimingPartitionedJournalStore store, params string[] entries)
    { var keys = new[] { "a" }; Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { entries })); }
    [Fact(Timeout = 10000)]
    public async Task SecondChunkRetirementStopsAfterOneWholeGroupRetry()
    {
        await Task.Yield(); var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore();
        var writer = OpenChunks(c, a, b); var second = OpenChunks(c, a, b); var reader = OpenChunks(c, a, b);
        PublishChunks(writer, "zero", "zero-tail", "zero-last");
        a.On("read", () => { PublishChunks(writer, "one", "one-tail", "one-last"); writer.CollectGarbage(); a.On("read", () => { PublishChunks(second, "two", "two-tail", "two-last"); second.CollectGarbage(); }); });
        var keys = new[] { "a" }; Assert.Throws<InvalidDataException>(() => reader.Read(keys)); Assert.Equal(new[] { "two", "two-tail", "two-last" }, reader.Read(keys)[0].Entries);
    }
    [Fact(Timeout = 10000)]
    public async Task OwnCollectorFencePreservesAllNewerOrphanChunks()
    {
        await Task.Yield(); var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore();
        var writer = OpenChunks(c, a, b); var collector = OpenChunks(c, a, b); var other = OpenChunks(c, a, b); PublishChunks(writer, "live", "tail", "last");
        c.On("scan", () =>
        {
            c.On("swap", () => throw new IOException("lost publication")); Assert.Throws<IOException>(() => PublishChunks(writer, "orphan", "tail", "last"));
            a.On("scan", () => throw new IOException("incomplete next collector")); Assert.Throws<IOException>(() => other.CollectGarbage());
        });
        Assert.Equal(new ReclamationResult(1, 4, 0), collector.CollectGarbage()); Assert.Equal(2, a.Values.Keys.Count(key => key.StartsWith("blob2-0000000000000001-", StringComparison.Ordinal)));
        Assert.Equal(2, collector.CollectGarbage()!.Deleted); Assert.Equal(new[] { "live", "tail", "last" }, collector.Read(new[] { "a" })[0].Entries);
    }
    [Fact(Timeout = 10000)]
    public async Task CompleteLaterNamespaceAndExactGenerationGuardChunkDeletion()
    {
        await Task.Yield(); var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore(); var store = OpenChunks(c, a, b);
        PublishChunks(store, "old", "tail", "last"); var key = c.Values["a"].Entries[4]; PublishChunks(store, "live", "fresh", "end");
        b.On("scan", () => throw new IOException("incomplete later namespace")); Assert.Throws<IOException>(() => store.CollectGarbage()); Assert.Equal(0, a.Count("delete"));
        a.On("delete", () => { a.Values[key] = new(++a.Revision, new[] { "ABA replacement" }); });
        Assert.Equal(1, store.CollectGarbage()!.Deleted); Assert.Equal(new[] { "ABA replacement" }, a.Values[key].Entries);
        Assert.Equal(new[] { "live", "fresh", "end" }, store.Read(new[] { "a" })[0].Entries);
    }
    [Fact(Timeout = 10000)]
    public async Task MixedWholeAndLiveEmptyChunkRootsPreserveFollowingEdits()
    {
        await Task.Yield(); var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore(); var legacy = Open(c, a, b);
        PublishChunks(legacy, "legacy"); var store = OpenChunks(c, a, b); var keys = new[] { "a" }; var owned = store.Read(keys);
        Assert.True(store.CompareExchange(keys, owned, new IReadOnlyList<string>?[] { Array.Empty<string>() })); Assert.Equal(new[] { "chunks2", "east", "0", "0" }, c.Values["a"].Entries);
        Assert.Equal(1, store.CollectGarbage()!.Deleted); Assert.Empty(a.Values); Assert.Empty(legacy.Read(keys)[0].Entries);
        Assert.False(store.CompareExchange(keys, owned, new IReadOnlyList<string>?[] { new[] { "stale" } })); PublishChunks(store, "following", "tail"); Assert.Equal(new[] { "following", "tail" }, legacy.Read(keys)[0].Entries);
    }
}
