using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public partial class PartitionedObjectCacheTests
{
    private sealed class Store : IAtomicJournalStore
    {
        internal readonly Dictionary<string, JournalVersion> Values = new();
        internal int Reads; internal bool Fail; internal bool LoseReply; internal Action? ReadHook;
        private ulong _revision;
        public bool SupportsDurableFlush => true;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys)
        {
            Interlocked.Increment(ref Reads); ReadHook?.Invoke(); if (Fail) throw new IOException("unavailable");
            return keys.Select(key => Values.TryGetValue(key, out var item) ? new(item.Generation, item.Entries.ToArray()) : new JournalVersion(0, Array.Empty<string>())).ToArray();
        }
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            if (Fail) throw new IOException("unavailable");
            if (keys.Where((key, i) => (Values.GetValueOrDefault(key)?.Generation ?? 0) != expected[i].Generation).Any()) return false;
            var revision = ++_revision;
            for (int i = 0; i < keys.Count; i++) if (updates[i] is { } data) Values[keys[i]] = new(revision, data.ToArray());
            if (LoseReply) throw new IOException("reply lost");
            return true;
        }
    }
    private static PartitionedJournalStore Open(Store catalog, Store east, Store west, long cacheBytes = 0) =>
        new(catalog, new Dictionary<string, IAtomicJournalStore> { ["east"] = east, ["west"] = west }, key => key == "a" ? "east" : "west", cacheBytes: cacheBytes);
    private static void Put(PartitionedJournalStore writer, string key, params string[] data)
    {
        var keys = new[] { key }; Assert.True(writer.CompareExchange(keys, writer.Read(keys), new IReadOnlyList<string>?[] { data }));
    }
    [Fact]
    public void CacheKeepsCatalogFreshAndReturnsIndependentArrays()
    {
        var catalog = new Store(); var east = new Store(); var west = new Store(); var writer = Open(catalog, east, west);
        Put(writer, "a", "é"); var reader = Open(catalog, east, west, 4); var keys = new[] { "a" }; var start = east.Reads;
        var first = reader.Read(keys); Assert.Equal(start + 1, east.Reads);
        Assert.IsType<string[]>(first[0].Entries)[0] = "caller mutation";
        east.Fail = true;
        Assert.Equal(new[] { "é" }, reader.Read(keys)[0].Entries); Assert.Equal(start + 1, east.Reads);
        east.Fail = false;
        Put(writer, "a", "x"); var next = reader.Read(keys);
        Assert.True(next[0].Generation > first[0].Generation); Assert.Equal(new[] { "x" }, next[0].Entries);
        Assert.False(reader.CompareExchange(keys, first, new IReadOnlyList<string>?[] { null }));
        catalog.Fail = true; Assert.Throws<IOException>(() => reader.Read(keys)); catalog.Fail = false;
        Assert.Equal(new[] { "x" }, reader.Read(keys)[0].Entries);
    }
    [Fact]
    public void ExactUtf8BudgetPromotesHitsAndEvictsLeastRecentlyUsed()
    {
        var catalog = new Store(); var east = new Store(); var west = new Store(); var writer = Open(catalog, east, west);
        Put(writer, "a", "é"); Put(writer, "b", "abc"); Put(writer, "c", "");
        var reader = Open(catalog, east, west, 9); var start = east.Reads + west.Reads;
        foreach (var key in new[] { "a", "b", "a", "c" }) reader.Read(new[] { key });
        Assert.Equal(start + 3, east.Reads + west.Reads);
        reader.Read(new[] { "b" }); Assert.Equal(start + 4, east.Reads + west.Reads);
        reader.Read(new[] { "c" }); Assert.Equal(start + 4, east.Reads + west.Reads);
        reader.Read(new[] { "a" }); Assert.Equal(start + 5, east.Reads + west.Reads);
    }
    [Theory]
    [InlineData(0, 2)]
    [InlineData(3, 2)]
    [InlineData(4, 1)]
    public void ZeroOversizeAndMismatchedGenerationsNeverHit(long budget, int downloads)
    {
        var catalog = new Store(); var east = new Store(); var west = new Store(); var writer = Open(catalog, east, west);
        Put(writer, "a", "é"); var reader = Open(catalog, east, west, budget); var keys = new[] { "a" }; var start = east.Reads;
        reader.Read(keys); reader.Read(keys); Assert.Equal(start + downloads, east.Reads);
        var parts = catalog.Values["a"].Entries.ToArray(); parts[3] = "99";
        catalog.Values["a"] = catalog.Values["a"] with { Entries = parts };
        Assert.Throws<InvalidDataException>(() => reader.Read(keys)); Assert.Throws<InvalidDataException>(() => reader.Read(keys));
        Assert.Equal(start + downloads + 2, east.Reads);
    }
}
