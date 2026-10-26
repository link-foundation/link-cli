using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public partial class ReclamationTests
{
    [Theory(Timeout = 10000)]
    [InlineData("catalog-scan")]
    [InlineData("east-scan")]
    [InlineData("west-scan")]
    [InlineData("delete")]
    public async Task ConcurrentPublicationAtEveryCollectionSeamRemainsReachable(string phase)
    {
        await Task.Yield(); var (c, a, b, writer) = Fixture(); var collector = Open(c, a, b); Write(writer, "old", "b0"); Write(writer, "current", "b1");
        var (provider, operation) = phase switch { "catalog-scan" => (c, "scan"), "east-scan" => (a, "scan"), "west-scan" => (b, "scan"), _ => (a, "delete") };
        var following = Open(c, a, b); provider.On(operation, () => Write(following, "concurrent", "b2"));
        Assert.Equal(1UL, collector.CollectGarbage()!.Epoch); Assert.Equal(new[] { "concurrent" }, writer.Read(Keys)[0].Entries);
        writer.CollectGarbage(); Assert.Equal(new[] { "b2" }, writer.Read(Keys)[1].Entries);
    }
    [Fact(Timeout = 10000)]
    public async Task EpochFenceImmediatelyBeforePublicationReturnsKnownConflict()
    {
        await Task.Yield(); var (c, a, b, writer) = Fixture(); var collector = Open(c, a, b); Write(writer, "old", "b");
        c.On("swap", () => collector.CollectGarbage()); var before = writer.Read(Keys);
        Assert.False(writer.CompareExchange(Keys, before, new IReadOnlyList<string>?[] { new[] { "in-flight" }, null }));
        Assert.Equal(new[] { "old" }, writer.Read(Keys)[0].Entries); Write(writer, "following", "b");
    }
    [Fact(Timeout = 10000)]
    public async Task OwnFenceExcludesNewOrphanAfterInterruptedCompetingCollection()
    {
        await Task.Yield(); var (c, a, b, writer) = Fixture(); var collector = Open(c, a, b); var other = Open(c, a, b); Write(writer, "live", "b");
        c.On("scan", () =>
        {
            c.On("swap", () => throw new IOException("Lost publication."));
            Assert.Throws<IOException>(() => writer.CompareExchange(new[] { "a" }, writer.Read(new[] { "a" }), new IReadOnlyList<string>?[] { new[] { "orphan at one" } }));
            a.On("scan", () => throw new IOException("Incomplete next collector.")); Assert.Throws<IOException>(() => other.CollectGarbage());
        });
        Assert.Equal(new ReclamationResult(1, 3, 0), collector.CollectGarbage()); Assert.Contains(a.Values.Keys, key => key.StartsWith("blob2-0000000000000001-", StringComparison.Ordinal));
        Assert.Equal(1, collector.CollectGarbage()!.Deleted); Assert.Equal(new[] { "live" }, writer.Read(Keys)[0].Entries);
    }
    [Fact(Timeout = 10000)]
    public async Task SecondRetirementStopsAfterOneWholeGroupRetry()
    {
        await Task.Yield(); var (c, a, b, writer) = Fixture(); var reader = Open(c, a, b); Write(writer, "zero", "b");
        a.On("read", () => { Write(writer, "one", "b"); writer.CollectGarbage(); a.On("read", () => { Write(writer, "two", "b"); writer.CollectGarbage(); }); });
        Assert.Throws<InvalidDataException>(() => reader.Read(Keys)); Assert.Equal(new[] { "two" }, reader.Read(Keys)[0].Entries);
    }
    [Fact(Timeout = 10000)]
    public async Task ResurrectionUsesNewEpochAndOldOwnedValuesCannotPublish()
    {
        await Task.Yield(); var (c, a, _, writer) = Fixture(); var old = Write(writer, "old", "b"); var oldKey = c.Values["a"].Entries[2];
        Write(writer, "new", "b"); Assert.Equal(1, writer.CollectGarbage()!.Deleted); Assert.False(a.Values.ContainsKey(oldKey));
        Assert.False(writer.CompareExchange(Keys, old, new IReadOnlyList<string>?[] { new[] { "stale" }, null })); Write(writer, "old", "b");
        var newKey = c.Values["a"].Entries[2]; Assert.NotEqual(oldKey, newKey); Assert.StartsWith("blob2-0000000000000001-", newKey, StringComparison.Ordinal); Assert.Equal(new[] { "old" }, old[0].Entries);
    }
    [Theory(Timeout = 10000)]
    [InlineData(false)]
    [InlineData(true)]
    public async Task DeletionResponseLossIsNeverReplayed(bool after)
    {
        await Task.Yield(); var (_, a, _, store) = Fixture(); Write(store, "old", "b"); Write(store, "new", "b"); a.On("delete", () => throw new IOException("Delete response lost."), after);
        Assert.Throws<IOException>(() => store.CollectGarbage()); Assert.Equal(1, a.Count("delete")); Assert.Equal(after ? 1 : 2, a.Values.Count);
        Assert.Equal(new[] { "new" }, store.Read(Keys)[0].Entries); Assert.Equal(after ? 0 : 1, store.CollectGarbage()!.Deleted);
    }
}
