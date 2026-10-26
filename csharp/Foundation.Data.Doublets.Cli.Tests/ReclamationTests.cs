using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public partial class ReclamationTests
{
    private const string EpochKey = "__link_gc_epoch";
    private static string[] Keys => new[] { "a", "b" };
    private static ReclaimingPartitionedJournalStore Open(ReclamationMemoryStore c, ReclamationMemoryStore a, ReclamationMemoryStore b, ReclamationOptions? options = null) =>
        ReclaimingPartitionedJournalStore.Open(c, new Dictionary<string, IJournalMaintenanceStore> { ["east"] = a, ["west"] = b }, key => key == "a" ? "east" : "west", options);
    private static (ReclamationMemoryStore Catalog, ReclamationMemoryStore A, ReclamationMemoryStore B, ReclaimingPartitionedJournalStore Store) Fixture()
    {
        var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore(); return (c, a, b, Open(c, a, b));
    }
    private static IReadOnlyList<JournalVersion> Write(ReclaimingPartitionedJournalStore store, string a, string b)
    {
        Assert.True(store.CompareExchange(Keys, store.Read(Keys), new IReadOnlyList<string>?[] { new[] { a }, new[] { b } })); return store.Read(Keys);
    }
    [Fact(Timeout = 10000)]
    public async Task ExplicitCollectionPreservesCurrentRootsOwnedValuesAndStaleGuards()
    {
        await Task.Yield(); var (c, a, b, store) = Fixture(); Assert.False(store.SupportsDurableFlush);
        Assert.Equal(new[] { "gc1", "0" }, c.Values[EpochKey].Entries);
        Assert.All(store.Read(Keys), item => { Assert.Equal(0UL, item.Generation); Assert.Empty(item.Entries); });
        var initial = Write(store, "a0 雪", "b0"); Write(store, "a1", "b1");
        Assert.Equal(2, a.Values.Count); Assert.Equal(2, b.Values.Count); Assert.Equal(0, a.Count("delete"));
        Assert.Equal(new ReclamationResult(1, 4, 2), store.CollectGarbage()); Assert.Single(a.Values); Assert.Single(b.Values);
        Assert.Equal(new[] { "a1" }, store.Read(Keys)[0].Entries); Assert.Equal(new[] { "a0 雪" }, initial[0].Entries);
        Assert.False(store.CompareExchange(Keys, initial, new IReadOnlyList<string>?[] { new[] { "stale" }, null }));
        Assert.Throws<ArgumentException>(() => store.Read(new[] { EpochKey }));
        Assert.Throws<ArgumentException>(() => store.Read(Enumerable.Range(0, 32).Select(index => $"s{index}").ToArray()));
    }
    [Fact(Timeout = 10000)]
    public async Task EpochFenceRejectsAnInFlightPublisherAfterStaging()
    {
        await Task.Yield(); var (c, a, b, writer) = Fixture(); var collector = Open(c, a, b); Write(writer, "a0", "b0");
        var before = writer.Read(new[] { "a" });
        a.On("swap", () => Assert.Equal(1UL, collector.CollectGarbage()!.Epoch), after: true);
        bool published;
        try { published = writer.CompareExchange(new[] { "a" }, before, new IReadOnlyList<string>?[] { new[] { "in-flight" } }); }
        catch (InvalidDataException) { published = false; }
        Assert.False(published); Assert.Equal(new[] { "a0" }, writer.Read(Keys)[0].Entries);
        Assert.True(writer.CompareExchange(new[] { "a" }, writer.Read(new[] { "a" }), new IReadOnlyList<string>?[] { new[] { "following" } }));
    }
    [Fact(Timeout = 10000)]
    public async Task MissingRetiredShardRefreshesTheEntireOwnedGroup()
    {
        await Task.Yield(); var (c, a, b, writer) = Fixture(); var reader = Open(c, a, b); Write(writer, "a0", "b0");
        b.On("read", () => { Write(writer, "a1", "b1"); Assert.Equal(2, writer.CollectGarbage()!.Deleted); });
        var group = reader.Read(Keys); Assert.Equal(new[] { "a1" }, group[0].Entries); Assert.Equal(new[] { "b1" }, group[1].Entries);
    }
    [Fact(Timeout = 10000)]
    public async Task IncompleteScanDeletesNothingAndLostDeletionIsNeverReplayed()
    {
        await Task.Yield(); var (_, a, b, store) = Fixture(); Write(store, "a0", "b0"); Write(store, "a1", "b1");
        b.On("scan", () => throw new IOException("Incomplete snapshot.")); Assert.Throws<IOException>(() => store.CollectGarbage());
        Assert.Equal(0, a.Count("delete")); Assert.Equal(0, b.Count("delete"));
        a.On("delete", () => throw new IOException("Delete response lost."), after: true); Assert.Throws<IOException>(() => store.CollectGarbage());
        Assert.Equal(1, a.Count("delete")); Assert.Single(a.Values); Assert.Equal(new[] { "a1" }, store.Read(Keys)[0].Entries);
        Assert.Equal(1, store.CollectGarbage()!.Deleted);
    }

    [Fact(Timeout = 10000)]
    public async Task AliasedProvidersPreserveEveryLiveRoot()
    {
        await Task.Yield(); var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var store = Open(c, a, a); var keys = new[] { "a" };
        Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { new[] { "live" } }));
        Assert.Equal(0, store.CollectGarbage()!.Deleted); Assert.Equal(new[] { "live" }, store.Read(keys)[0].Entries);
    }
}
