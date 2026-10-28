using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public partial class ReclamationTests
{
    [Fact(Timeout = 10000)]
    public async Task UnsupportedChunkManifestsNeverReachDeletion()
    {
        await Task.Yield();
        foreach (var entries in new[] { new[] { "chunks1", "east", "0", "0" }, new[] { "chunks1", "east", "1", "1", "blob1-0000000000000000", "1", "1" } })
        {
            var (c, a, b, store) = Fixture(); Write(store, "a0", "b0"); Write(store, "a1", "b1");
            var beforeA = a.Values.ToArray(); var beforeB = b.Values.ToArray();
            c.Values["a"] = c.Values["a"] with { Entries = entries };
            Assert.Throws<InvalidDataException>(() => store.CollectGarbage());
            Assert.Equal(0, Deletions(a, b)); Assert.Equal(beforeA, a.Values.ToArray()); Assert.Equal(beforeB, b.Values.ToArray());
        }
    }
    [Fact(Timeout = 10000)]
    public async Task MalformedCompleteSnapshotsDeleteNothingInAnyPartition()
    {
        await Task.Yield();
        for (var phase = 0; phase < 3; phase++) foreach (var damage in new[] { "empty", "zero", "unordered", "duplicate", "name", "format", "entries" })
        {
            var (c, a, b, store) = Fixture(); Write(store, "a0", "b0"); Write(store, "a1", "b1"); var provider = new[] { c, a, b }[phase];
            JournalNamespaceEntry Entry(string key, ulong generation, params string[] entries) => new(key, new(generation, entries));
            provider.ScanOverride = damage switch
            {
                "empty" => Array.Empty<JournalNamespaceEntry>(),
                "zero" => new[] { Entry("a", 0) },
                "unordered" => new[] { Entry("z", 1), Entry("a", 2) },
                "duplicate" => new[] { Entry("a", 1), Entry("a", 1) },
                "name" => new[] { Entry("../bad", 1) },
                "format" => new[] { Entry("unrecognized", 1) },
                _ => new[] { Entry(phase == 0 ? EpochKey : "a", 1, "extra") }
            };
            Assert.NotNull(Record.Exception(() => store.CollectGarbage())); Assert.Equal(0, Deletions(a, b));
        }
    }
    [Theory(Timeout = 10000)]
    [InlineData("contradictory")]
    [InlineData("generation")]
    [InlineData("absent")]
    [InlineData("future")]
    [InlineData("old-snapshot")]
    public async Task ContradictoryRootsAndChangedProtectedGenerationsReject(string damage)
    {
        await Task.Yield(); var (c, a, b, store) = Fixture(); Write(store, "a0", "b0"); Write(store, "a1", "b1"); var pointer = c.Values["a"];
        switch (damage)
        {
            case "contradictory": var bad = pointer.Entries.ToArray(); bad[3] = "999"; c.Values["z"] = new(pointer.Generation, bad); break;
            case "generation": var data = a.Values[pointer.Entries[2]]; a.Values[pointer.Entries[2]] = new(999, data.Entries); break;
            case "absent": a.Values.Remove(pointer.Entries[2]); break;
            case "future": bad = pointer.Entries.ToArray(); bad[2] = "blob2-ffffffffffffffff-" + bad[2][23..]; c.Values["a"] = new(pointer.Generation, bad); break;
            default: c.ScanOverride = c.ScanNamespace(); break;
        }
        Assert.Throws<InvalidDataException>(() => store.CollectGarbage()); Assert.Equal(0, Deletions(a, b));
    }
    [Fact(Timeout = 10000)]
    public async Task ProviderCannotBypassSelectedScanLimits()
    {
        await Task.Yield(); foreach (var options in new[] { new ReclamationOptions(MaxKeys: 1), new(MaxBytes: 32) })
        {
            var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore(); var store = Open(c, a, b, options); Write(store, "a0", "b0");
            c.ScanOverride = c.ScanNamespace(); Assert.Throws<InvalidDataException>(() => store.CollectGarbage()); Assert.Equal(0, Deletions(a, b));
        }
    }
    [Fact(Timeout = 10000)]
    public async Task OwnershipAndStagingCollisionChecksPreserveCatalog()
    {
        await Task.Yield(); var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore();
        a.Values["foreign"] = new(1, Array.Empty<string>()); Assert.Throws<InvalidDataException>(() => Open(c, a, b)); Assert.Equal(0, c.Count("swap"));
        a.Values.Clear(); c.Values["a"] = new(1, new[] { "blob1", "east", "legacy", "1" }); Assert.Throws<InvalidDataException>(() => Open(c, a, b)); Assert.Equal(0, c.Count("swap"));
        var fixture = Fixture(); c = fixture.Catalog; a = fixture.A; var store = fixture.Store; var expected = Write(store, "live", "b"); var count = a.Count("swap");
        Assert.True(store.CompareExchange(Keys, expected, new IReadOnlyList<string>?[] { new[] { "live" }, null })); Assert.Equal(count, a.Count("swap"));
        var key = a.Values.Keys.Single(); var data = a.Values[key]; a.Values[key] = new(data.Generation, new[] { "collision" }); count = c.Count("swap");
        Assert.Throws<InvalidDataException>(() => store.CompareExchange(new[] { "a" }, new[] { c.Values["a"] }, new IReadOnlyList<string>?[] { new[] { "live" } })); Assert.Equal(count, c.Count("swap"));
    }

    [Fact(Timeout = 10000)]
    public async Task ChangedCatalogPointerCannotReuseGenerationDuringRefresh()
    {
        await Task.Yield(); var (c, a, b, writer) = Fixture(); var reader = Open(c, a, b); Publish(writer, "old"); var generation = c.Values["a"].Generation;
        a.On("read", () => { Publish(writer, "new"); c.Values["a"] = new(generation, c.Values["a"].Entries); writer.CollectGarbage(); });
        Assert.Throws<InvalidDataException>(() => reader.Read(new[] { "a" }));
    }
}
