using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public partial class ReclamationTests
{
    [Fact(Timeout = 10000)]
    public async Task InvalidPartitionConfigurationPerformsNoIo()
    {
        await Task.Yield();
        foreach (var name in new[] { null, "", "bad/name", new string('a', 129) })
        {
            var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var partitions = new Dictionary<string, IJournalMaintenanceStore>();
            if (name is not null) partitions.Add(name, a);
            Assert.Throws<ArgumentException>(() => ReclaimingPartitionedJournalStore.Open(c, partitions, _ => "east"));
            Assert.Empty(c.Calls); Assert.Empty(a.Calls);
        }
    }
    [Fact(Timeout = 10000)]
    public async Task AllRoutesValidateBeforeIoAndReadonlyDependenciesNeedNoRoute()
    {
        await Task.Yield(); var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore();
        var store = ReclaimingPartitionedJournalStore.Open(c, new Dictionary<string, IJournalMaintenanceStore> { ["east"] = a }, key => key == "a" ? "east" : "unconfigured");
        var expected = store.Read(Keys); var counts = (c.Calls.Count, a.Calls.Count);
        Assert.Throws<ArgumentException>(() => store.CompareExchange(Keys, expected, new IReadOnlyList<string>?[] { new[] { "first" }, new[] { "second" } }));
        Assert.Equal(counts, (c.Calls.Count, a.Calls.Count));
        Assert.True(store.CompareExchange(Keys, expected, new IReadOnlyList<string>?[] { null, null })); Assert.Empty(a.Values);
        Assert.True(store.CompareExchange(Keys, expected, new IReadOnlyList<string>?[] { new[] { "following" }, null }));
        Assert.Equal(new[] { "following" }, store.Read(Keys)[0].Entries);
    }
    [Fact(Timeout = 10000)]
    public async Task DurabilityRequiresEveryProviderWithoutSendingRequests()
    {
        await Task.Yield();
        for (var mask = 0; mask < 8; mask++)
        {
            var (c, a, b, store) = Fixture(); var providers = new[] { c, a, b };
            for (var bit = 0; bit < providers.Length; bit++) providers[bit].SupportsDurableFlush = (mask & (1 << bit)) != 0;
            var counts = providers.Select(provider => provider.Calls.Count).ToArray();
            Assert.Equal(mask == 7, store.SupportsDurableFlush); Assert.Equal(counts, providers.Select(provider => provider.Calls.Count));
        }
    }
    [Fact(Timeout = 10000)]
    public async Task FailedInitializationConfirmationPreservesOneAcknowledgedDecision()
    {
        await Task.Yield(); var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore();
        c.On("swap", () => c.On("scan", () => throw new IOException("Initialization confirmation unavailable.")), after: true);
        Assert.Throws<IOException>(() => Open(c, a, b)); Assert.Equal(1, c.Count("swap")); Assert.Equal(new[] { "gc1", "0" }, c.Values[EpochKey].Entries);
        var fresh = Open(c, a, b); Assert.All(fresh.Read(Keys), item => Assert.Equal(0UL, item.Generation)); Assert.Equal(1, c.Count("swap"));
    }
    [Fact(Timeout = 10000)]
    public async Task FailedStagingConfirmationLeavesCollectableOrphanWithoutPublication()
    {
        await Task.Yield(); var (c, a, _, store) = Fixture(); var keys = new[] { "a" }; var expected = store.Read(keys);
        a.On("swap", () => a.On("read", () => throw new IOException("Staging confirmation unavailable.")), after: true);
        Assert.Throws<IOException>(() => store.CompareExchange(keys, expected, new IReadOnlyList<string>?[] { new[] { "orphan" } }));
        Assert.Equal(1, a.Count("swap")); Assert.Single(a.Values); Assert.Equal(1, c.Count("swap")); Assert.False(c.Values.ContainsKey("a"));
        Assert.Equal(0UL, store.Read(keys)[0].Generation); Assert.Equal(1, store.CollectGarbage()!.Deleted); Assert.Empty(a.Values); Write(store, "following", "b");
    }
    private static void InterruptedRefresh(string phase)
    {
        var (c, a, b, writer) = Fixture(); var reader = Open(c, a, b); Write(writer, "old-a", "old-b"); var provider = phase == "catalog" ? c : a;
        b.On("read", () => { Write(writer, "new-a", "new-b"); Assert.Equal(2, writer.CollectGarbage()!.Deleted); provider.On("read", () => throw new IOException("Refresh unavailable.")); });
        Assert.Throws<IOException>(() => reader.Read(Keys)); Assert.Equal(2, a.Count("delete") + b.Count("delete"));
        var fresh = reader.Read(Keys); Assert.Equal(new[] { "new-a" }, fresh[0].Entries); Assert.Equal(new[] { "new-b" }, fresh[1].Entries);
    }
    [Fact(Timeout = 10000)]
    public async Task InterruptedCatalogRefreshReturnsNoPartialGroup() { await Task.Yield(); InterruptedRefresh("catalog"); }
    [Fact(Timeout = 10000)]
    public async Task InterruptedRefreshedObjectReturnsNoPartialGroup() { await Task.Yield(); InterruptedRefresh("data"); }
}
