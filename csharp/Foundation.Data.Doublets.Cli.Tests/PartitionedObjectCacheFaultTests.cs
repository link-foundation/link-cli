using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public partial class PartitionedObjectCacheTests
{
    [Fact]
    public void OriginalThreeArgumentConstructorRetainsItsBinarySignatureAndNoCache()
    {
        var constructor = typeof(PartitionedJournalStore).GetConstructor(new[]
        {
            typeof(IAtomicJournalStore), typeof(IReadOnlyDictionary<string, IAtomicJournalStore>), typeof(Func<string, string>)
        });
        Assert.NotNull(constructor);
        var catalog = new Store(); var east = new Store(); var west = new Store(); var writer = Open(catalog, east, west);
        Put(writer, "a", "owned");
        var reader = Assert.IsType<PartitionedJournalStore>(constructor.Invoke(new object[]
        {
            catalog, new Dictionary<string, IAtomicJournalStore> { ["east"] = east, ["west"] = west }, (Func<string, string>)(_ => "east")
        }));
        var start = east.Reads; reader.Read(new[] { "a" }); reader.Read(new[] { "a" }); Assert.Equal(start + 2, east.Reads);
    }
    [Fact]
    public void FiniteIndependentLruOracleMatchesUnicodePayloadCosts()
    {
        string[][] rows = { Array.Empty<string>(), new[] { "" }, new[] { "é" }, new[] { "abc" }, new[] { "雪" }, new[] { "😀" } };
        int[] costs = { 1, 2, 4, 5, 5, 6 }; string[] ids = { "a", "b", "c", "d", "e", "f" };
        foreach (var budget in new[] { 0, 1, 2, 4, 9, 17 })
        {
            var catalog = new Store(); var east = new Store(); var west = new Store(); var writer = Open(catalog, east, west);
            for (int i = 0; i < ids.Length; i++) Put(writer, ids[i], rows[i]);
            var reader = Open(catalog, east, west, budget); var start = east.Reads + west.Reads;
            var order = new List<int>(); int used = 0; int misses = 0; uint seed = 138;
            for (int step = 0; step < 128; step++)
            {
                seed = unchecked(seed * 1664525 + 1013904223); int selected = (int)((seed >> 16) % 6);
                if (order.Remove(selected)) order.Add(selected);
                else
                {
                    misses++;
                    if (costs[selected] <= budget)
                    {
                        while (used + costs[selected] > budget) { used -= costs[order[0]]; order.RemoveAt(0); }
                        used += costs[selected]; order.Add(selected);
                    }
                }
                Assert.Equal(rows[selected], reader.Read(new[] { ids[selected] })[0].Entries);
                Assert.Equal(start + misses, east.Reads + west.Reads);
            }
        }
    }
    [Theory]
    [InlineData(1, 3)]
    [InlineData(2, 2)]
    public void EmptyObjectsHavePositiveWeightAndPartitionsNeverAlias(long budget, int downloads)
    {
        var catalog = new Store(); var east = new Store(); var west = new Store(); var writer = Open(catalog, east, west);
        Put(writer, "a"); Put(writer, "b"); Put(writer, "c", "é");
        var reader = Open(catalog, east, west, budget); var start = east.Reads + west.Reads;
        foreach (var key in new[] { "a", "b", "a" }) reader.Read(new[] { key });
        Assert.Equal(start + downloads, east.Reads + west.Reads);
        reader.Read(new[] { "c" }); reader.Read(new[] { "a" });
        Assert.Equal(start + downloads + 1, east.Reads + west.Reads);
        catalog.Values["a"] = catalog.Values["a"] with { Generation = 99 };
        Assert.Equal(99UL, reader.Read(new[] { "a" })[0].Generation);
        catalog.Values.Remove("a");
        var absent = reader.Read(new[] { "a" })[0]; Assert.Equal(0UL, absent.Generation); Assert.Empty(absent.Entries);
    }
    [Fact]
    public void FailedFetchCanBeRepairedAndStagingAlwaysComparesCompleteContents()
    {
        var catalog = new Store(); var east = new Store(); var west = new Store(); var writer = Open(catalog, east, west);
        Put(writer, "a", "é"); var item = east.Values.Single(); var reader = Open(catalog, east, west, 4); var keys = new[] { "a" }; var start = east.Reads;
        east.Values[item.Key] = item.Value with { Entries = new[] { "broken" } };
        Assert.Throws<InvalidDataException>(() => reader.Read(keys));
        east.Values[item.Key] = item.Value; var expected = reader.Read(keys); reader.Read(keys);
        Assert.Equal(start + 2, east.Reads);
        east.Values[item.Key] = item.Value with { Entries = new[] { "broken" } };
        Assert.Throws<InvalidDataException>(() => reader.CompareExchange(keys, expected, new IReadOnlyList<string>?[] { new[] { "é" } }));
        Assert.Equal(expected[0].Generation, catalog.Values["a"].Generation);
        east.Values[item.Key] = item.Value;
        Assert.True(reader.CompareExchange(keys, expected, new IReadOnlyList<string>?[] { new[] { "é" } }));
        Assert.NotEqual(expected[0].Generation, reader.Read(keys)[0].Generation);
    }
    [Fact]
    public void RejectedAndUncertainPublicationsResolveFromFreshCatalogState()
    {
        var catalog = new Store(); var east = new Store(); var west = new Store(); var writer = Open(catalog, east, west);
        Put(writer, "a", "old"); var reader = Open(catalog, east, west, 20); var keys = new[] { "a" }; var old = reader.Read(keys);
        Put(writer, "a", "new"); Assert.False(reader.CompareExchange(keys, old, new IReadOnlyList<string>?[] { new[] { "orphan" } }));
        var current = reader.Read(keys); Assert.Equal(new[] { "new" }, current[0].Entries);
        var update = new IReadOnlyList<string>?[] { new[] { "next" } };
        catalog.Fail = true; Assert.Throws<IOException>(() => reader.CompareExchange(keys, current, update));
        catalog.Fail = false; Assert.Equal(new[] { "new" }, reader.Read(keys)[0].Entries);
        catalog.LoseReply = true; Assert.Throws<IOException>(() => reader.CompareExchange(keys, current, update));
        catalog.LoseReply = false; Assert.Equal(new[] { "next" }, reader.Read(keys)[0].Entries);
    }
    [Fact]
    public async Task ConcurrentMissesRetainAndAccountForOneOwnedCopy()
    {
        var catalog = new Store(); var east = new Store(); var west = new Store(); var writer = Open(catalog, east, west);
        Put(writer, "a", ""); Put(writer, "b"); var reader = Open(catalog, east, west, 3); var start = east.Reads + west.Reads;
        using var barrier = new Barrier(2);
        east.ReadHook = () => Assert.True(barrier.SignalAndWait(TimeSpan.FromSeconds(5)), "concurrent data reads must reach the barrier");
        IReadOnlyList<JournalVersion>[] reads;
        try
        {
            reads = await Task.WhenAll(Task.Run(() => reader.Read(new[] { "a" })), Task.Run(() => reader.Read(new[] { "a" }))).WaitAsync(TimeSpan.FromSeconds(10));
        }
        finally { east.ReadHook = null; }
        Assert.Equal(start + 2, east.Reads + west.Reads);
        Assert.IsType<string[]>(reads[0][0].Entries)[0] = "caller"; Assert.Equal(new[] { "" }, reads[1][0].Entries);
        foreach (var key in new[] { "b", "a", "b" }) reader.Read(new[] { key });
        Assert.Equal(start + 3, east.Reads + west.Reads);
    }
    [Fact]
    public void NegativeBudgetFailsBeforeStorageAccess()
    {
        var catalog = new Store(); var east = new Store(); var west = new Store();
        Assert.Throws<ArgumentOutOfRangeException>(() => Open(catalog, east, west, -1));
        Assert.Equal(0, catalog.Reads + east.Reads + west.Reads);
    }
}
