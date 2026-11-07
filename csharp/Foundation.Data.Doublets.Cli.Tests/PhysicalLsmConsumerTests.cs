using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class PhysicalLsmConsumerTests
{
    private static readonly LsmStoreOptions Options = new(new LsmOptions(128, 65536, 32, 16384, 2, 4, 262144), 16);
    private static PhysicalLsmJournalStore Open(PhysicalLsmHarness h) => PhysicalLsmJournalStore.Open(h.Wal(), h.Tables, Options);
    [Fact]
    public void ActualClusterLinksNamesStaleGuardsAndPhysicalReopen()
    {
        var h = new PhysicalLsmHarness();
        using (var store = Open(h))
        {
            var cluster = new ClusterEngine<uint>(store, new() { RequireDurableFlush = true });
            using (var seed = cluster.Begin("a", "b"))
            { seed.Shard("a").CreateLink(0, 0); seed.Shard("a").SetName(1, "雪"); seed.Shard("b").CreateLink(0, 0); seed.Commit(); }
            using var stale = cluster.Begin("a", "b");
            using (var winner = cluster.Begin("b")) { winner.Shard("b").SetName(1, "fresh"); winner.Commit(); }
            stale.Shard("a").DeleteLink(1); Assert.Contains("conflict", Assert.Throws<InvalidOperationException>(() => stale.Commit()).Message, StringComparison.OrdinalIgnoreCase);
            store.Compact();
        }
        var fresh = new PhysicalLsmHarness(h.Image()); using var reopened = Open(fresh); var again = new ClusterEngine<uint>(reopened);
        using var check = again.Begin("a", "b"); Assert.NotNull(check.Shard("a").GetLink(1)); Assert.Equal("雪", check.Shard("a").GetName(1)); Assert.Equal("fresh", check.Shard("b").GetName(1));
    }
    [Fact]
    public void PartitionedCatalogsComposeActualPhysicalStoresAndReopen()
    {
        var h = new[] { new PhysicalLsmHarness(), new PhysicalLsmHarness(), new PhysicalLsmHarness() }; var keys = new[] { "a", "b" };
        using (var catalog = Open(h[0])) using (var east = Open(h[1])) using (var west = Open(h[2]))
        {
            var store = new PartitionedJournalStore(catalog, new Dictionary<string, IAtomicJournalStore> { { "east", east }, { "west", west } }, key => key == "a" ? "east" : "west");
            Assert.True(store.SupportsDurableFlush); Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { new[] { "雪" }, Array.Empty<string>() }));
            var stale = store.Read(keys); Assert.True(store.CompareExchange(new[] { "a" }, new[] { stale[0] }, new IReadOnlyList<string>?[] { new[] { "new" } }));
            Assert.False(store.CompareExchange(keys, stale, new IReadOnlyList<string>?[] { null, new[] { "lost" } })); catalog.Compact(); east.Compact(); west.Compact();
        }
        var fresh = h.Select(h => new PhysicalLsmHarness(h.Image())).ToArray();
        using var c = Open(fresh[0]); using var e = Open(fresh[1]); using var w = Open(fresh[2]);
        var again = new PartitionedJournalStore(c, new Dictionary<string, IAtomicJournalStore> { { "east", e }, { "west", w } }, key => key == "a" ? "east" : "west");
        Assert.Equal(new[] { "new" }, again.Read(keys)[0].Entries); Assert.Empty(again.Read(keys)[1].Entries);
    }
    [Fact]
    public void EpochCollectionUsesPhysicalScansAndGenerationTombstones()
    {
        var h = new[] { new PhysicalLsmHarness(), new PhysicalLsmHarness(), new PhysicalLsmHarness() }; var keys = new[] { "a", "b" };
        using (var catalog = Open(h[0])) using (var east = Open(h[1])) using (var west = Open(h[2]))
        {
            var store = ReclaimingPartitionedJournalStore.Open(catalog, new Dictionary<string, IJournalMaintenanceStore> { { "east", east }, { "west", west } }, key => key == "a" ? "east" : "west");
            Assert.True(store.SupportsDurableFlush);
            for (int i = 0; i < 2; i++) Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { new[] { "a" + i }, new[] { "b" + i } }));
            Assert.Equal(new ReclamationResult(1, 4, 2), store.CollectGarbage()); catalog.Compact(); east.Compact(); west.Compact();
        }
        var fresh = h.Select(h => new PhysicalLsmHarness(h.Image())).ToArray();
        using var c = Open(fresh[0]); using var e = Open(fresh[1]); using var w = Open(fresh[2]);
        var again = ReclaimingPartitionedJournalStore.Open(c, new Dictionary<string, IJournalMaintenanceStore> { { "east", e }, { "west", w } }, key => key == "a" ? "east" : "west");
        Assert.Equal(new[] { "a1" }, again.Read(keys)[0].Entries); Assert.Equal(new[] { "b1" }, again.Read(keys)[1].Entries); Assert.Equal(0, again.CollectGarbage()!.Deleted);
    }
}
