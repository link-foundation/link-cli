using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public partial class ReclamationTests
{
    private static void Publish(ReclaimingPartitionedJournalStore store, string text)
    {
        var keys = new[] { "a" }; Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { new[] { text } })); Assert.Equal(new[] { text }, store.Read(keys)[0].Entries);
    }
    private static int Deletions(ReclamationMemoryStore a, ReclamationMemoryStore b) => a.Count("delete") + b.Count("delete");
    [Theory(Timeout = 10000)]
    [InlineData(false)]
    [InlineData(true)]
    public async Task UnknownInitializationStagePublicationAndFenceAreNeverReplayed(bool after)
    {
        await Task.Yield(); var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore();
        c.On("swap", () => throw new IOException("Initialization response lost."), after);
        Assert.Throws<IOException>(() => Open(c, a, b)); Assert.Equal(1, c.Count("swap")); Assert.Equal(after, c.Values.ContainsKey(EpochKey)); Assert.Equal(0, Deletions(a, b)); Assert.NotNull(Open(c, a, b));
        foreach (var operation in new[] { "stage", "publish", "fence" })
        {
            var (catalog, east, west, store) = Fixture(); Publish(store, "old"); var provider = operation == "stage" ? east : catalog; var count = provider.Count("swap");
            provider.On("swap", () => throw new IOException("Response lost."), after);
            if (operation == "fence") Assert.Throws<IOException>(() => store.CollectGarbage());
            else Assert.Throws<IOException>(() => store.CompareExchange(new[] { "a" }, store.Read(new[] { "a" }), new IReadOnlyList<string>?[] { new[] { "new" } }));
            Assert.Equal(count + 1, provider.Count("swap")); Assert.Equal(0, Deletions(east, west));
            Assert.Equal(new[] { operation == "publish" && after ? "new" : "old" }, store.Read(new[] { "a" })[0].Entries); Publish(store, "following");
        }
    }
    [Fact(Timeout = 10000)]
    public async Task KnownCompetingFencesAndExhaustionPreserveOutcomes()
    {
        await Task.Yield(); var (c, a, b, store) = Fixture(); var other = Open(c, a, b);
        c.On("swap", () => Assert.Equal(1UL, other.CollectGarbage()!.Epoch)); Assert.Null(store.CollectGarbage()); Assert.Equal(0, Deletions(a, b));
        c.Values[EpochKey] = new(c.Values[EpochKey].Generation, new[] { "gc1", "18446744073709551615" }); var count = c.Count("swap");
        Assert.Throws<OverflowException>(() => store.CollectGarbage()); Assert.Equal(count, c.Count("swap"));
        c = new(); a = new(); b = new(); var competitor = c;
        c.On("swap", () => { competitor.Revision = 1; competitor.Values[EpochKey] = new(1, new[] { "gc1", "7" }); });
        store = Open(c, a, b); Assert.Equal(8UL, store.CollectGarbage()!.Epoch);
    }
    [Fact(Timeout = 10000)]
    public async Task ExactDeletionSkipsAnAbaReplacement()
    {
        await Task.Yield(); var (_, a, _, store) = Fixture(); Publish(store, "old"); Publish(store, "new"); var key = a.Values.Single(item => item.Value.Entries.SequenceEqual(new[] { "old" })).Key;
        a.On("delete", () => { a.Revision++; a.Values[key] = new(a.Revision, new[] { "ABA replacement" }); });
        Assert.Equal(0, store.CollectGarbage()!.Deleted); Assert.Equal(new[] { "ABA replacement" }, a.Values[key].Entries); Assert.Equal(new[] { "new" }, store.Read(new[] { "a" })[0].Entries);
    }
    [Theory(Timeout = 10000)]
    [InlineData("absent")]
    [InlineData("generation")]
    [InlineData("content")]
    [InlineData("transport")]
    [InlineData("short")]
    public async Task ReaderDamageFailsClosedAndOnlyAbsenceRefreshes(string damage)
    {
        await Task.Yield(); var (c, a, b, store) = Fixture(); Publish(store, "live"); var (key, value) = a.Values.Single();
        switch (damage)
        {
            case "absent": a.Values.Clear(); break;
            case "generation": a.Values[key] = new(999, value.Entries); break;
            case "content": a.Values[key] = new(value.Generation, new[] { "changed" }); break;
            case "transport": a.On("read", () => throw new IOException("Data outage.")); break;
            case "short": a.ReadOverride = Array.Empty<JournalVersion>(); break;
        }
        var count = c.Count("read"); Assert.NotNull(Record.Exception(() => store.Read(new[] { "a" })));
        Assert.Equal(damage == "absent" ? 2 : 1, c.Count("read") - count); Assert.Equal(0, Deletions(a, b));
    }
    [Fact(Timeout = 10000)]
    public async Task CachePreservesOwnedDataButRequiresLiveCatalog()
    {
        await Task.Yield(); var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore(); var store = Open(c, a, b, new(CacheBytes: 64));
        Publish(store, "old"); var owned = store.Read(new[] { "a" }); Publish(store, "new"); Assert.Equal(1, store.CollectGarbage()!.Deleted);
        a.On("read", () => throw new IOException("Data outage.")); Assert.Equal(new[] { "new" }, store.Read(new[] { "a" })[0].Entries); Assert.Equal(new[] { "old" }, owned[0].Entries);
        c.On("read", () => throw new IOException("Catalog outage.")); Assert.Throws<IOException>(() => store.Read(new[] { "a" }));
    }
    [Fact(Timeout = 10000)]
    public async Task ValidationPrecedesIoAndReadonlyGroupsReserveEpochSlot()
    {
        await Task.Yield(); var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore();
        foreach (var options in new[] { new ReclamationOptions(MaxKeys: 0), new(MaxBytes: 0), new(PageKeys: 0), new(PageKeys: 257), new(CacheBytes: -1) }) Assert.Throws<ArgumentOutOfRangeException>(() => Open(c, a, b, options));
        Assert.Equal(0, c.Count("scan")); var store = Open(c, a, b); var keys = Enumerable.Range(0, 31).Select(index => $"s{index}").ToArray();
        Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[31])); Assert.Equal(32, c.Calls[^1].Keys.Length); Assert.Empty(a.Values);
        var stale = store.Read(new[] { "a" }); Publish(store, "new"); var count = a.Count("read");
        Assert.False(store.CompareExchange(new[] { "a" }, stale, new IReadOnlyList<string>?[] { new[] { "orphan" } })); Assert.Equal(count, a.Count("read"));
        count = c.Count("read"); Assert.Throws<ArgumentException>(() => store.CompareExchange(new[] { "a" }, Array.Empty<JournalVersion>(), new IReadOnlyList<string>?[] { null })); Assert.Equal(count, c.Count("read")); Assert.Equal(0, Deletions(a, b));
    }
}
