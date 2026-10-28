using Foundation.Data.Doublets.Cli.Engine;
using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Tests;

public class PartitionedJournalStoreTests
{
    private sealed class Store : IAtomicJournalStore
    {
        public readonly Dictionary<string, JournalVersion> Values = new();
        private ulong _revision;
        public bool Fail; public bool LoseReply;
        public bool SupportsDurableFlush { get; set; } = true;
        public Func<IReadOnlyList<string>, IReadOnlyList<JournalVersion>>? ReadOverride;
        public Func<IReadOnlyList<string>, IReadOnlyList<JournalVersion>, IReadOnlyList<IReadOnlyList<string>?>, bool>? ExchangeOverride;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys)
        {
            if (ReadOverride is not null) return ReadOverride(keys);
            if (Fail) throw new IOException("Partition unavailable.");
            return keys.Select(key => Values.TryGetValue(key, out var item) ? new(item.Generation, item.Entries.ToArray()) : new JournalVersion(0, Array.Empty<string>())).ToArray();
        }
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            if (ExchangeOverride is not null) return ExchangeOverride(keys, expected, updates);
            if (Fail) throw new IOException("Partition unavailable.");
            if (keys.Where((key, i) => (Values.GetValueOrDefault(key)?.Generation ?? 0) != expected[i].Generation).Any()) return false;
            var generation = ++_revision;
            for (var i = 0; i < keys.Count; i++) if (updates[i] is { } data) Values[keys[i]] = new(generation, data.ToArray());
            if (LoseReply) throw new IOException("Reply lost.");
            return true;
        }
    }
    private static (Store Catalog, Store A, Store B, PartitionedJournalStore Store) Fixture()
    {
        var catalog = new Store(); var a = new Store(); var b = new Store();
        return (catalog, a, b, new(catalog, new Dictionary<string, IAtomicJournalStore> { ["east"] = a, ["west"] = b }, key => key == "a" ? "east" : "west"));
    }
    [Fact]
    public void PortableAddressesFrameUtf8ByteLengthsAndEntryBoundaries()
    {
        using var vectors = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "partitioned-journal-vectors.json")));
        foreach (var vector in vectors.RootElement.EnumerateArray())
        {
            var (_, a, _, store) = Fixture(); var keys = new[] { "a" };
            var entries = vector.GetProperty("entries").EnumerateArray().Select(value => value.GetString()!).ToArray();
            Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { entries }));
            Assert.Equal(vector.GetProperty("address").GetString(), Assert.Single(a.Values).Key);
            Assert.Equal(entries, store.Read(keys)[0].Entries);
        }
    }
    [Fact]
    public void IndependentObjectsUseSmallAtomicPointersAndValidateReadOnlyShards()
    {
        var (catalog, a, b, store) = Fixture(); var keys = new[] { "a", "b" }; var snapshot = store.Read(keys);
        Assert.True(store.SupportsDurableFlush);
        Assert.True(store.CompareExchange(keys, snapshot, new IReadOnlyList<string>?[] { new[] { "journal-é" }, new[] { "second" } }));
        Assert.Single(a.Values); Assert.Single(b.Values); Assert.DoesNotContain("journal-é", catalog.Values.Values.SelectMany(item => item.Entries));
        var stale = store.Read(keys); Assert.Equal(new[] { "journal-é" }, stale[0].Entries);
        Assert.True(store.CompareExchange(new[] { "b" }, new[] { stale[1] }, new IReadOnlyList<string>?[] { new[] { "changed" } }));
        Assert.False(store.CompareExchange(keys, stale, new IReadOnlyList<string>?[] { new[] { "abandoned" }, null }));
        Assert.Equal(new[] { "journal-é" }, store.Read(keys)[0].Entries);
        var count = a.Values.Count;
        Assert.True(store.CompareExchange(new[] { "a" }, store.Read(new[] { "a" }), new IReadOnlyList<string>?[] { new[] { "abandoned" } })); Assert.Equal(count, a.Values.Count);
    }
    [Fact]
    public void FailureStagesOrphansWithoutPublicationAndLostReplyIsDiscovered()
    {
        var (catalog, _, b, store) = Fixture(); var keys = new[] { "a", "b" }; var snapshot = store.Read(keys);
        var updates = new IReadOnlyList<string>?[] { new[] { "one" }, new[] { "two" } };
        b.Fail = true; Assert.Throws<IOException>(() => store.CompareExchange(keys, snapshot, updates)); Assert.Empty(catalog.Values);
        b.Fail = false; catalog.LoseReply = true; Assert.Throws<IOException>(() => store.CompareExchange(keys, snapshot, updates));
        Assert.Equal(new[] { "one" }, store.Read(keys)[0].Entries); Assert.Equal(new[] { "two" }, store.Read(keys)[1].Entries);
        b.Fail = true; Assert.Throws<IOException>(() => store.Read(keys));
    }
    [Fact]
    public void MissingOrModifiedDataAndInvalidPointersFailClosed()
    {
        var (catalog, a, _, store) = Fixture(); var keys = new[] { "a" };
        Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { new[] { "one" } }));
        var (key, data) = a.Values.Single(); a.Values.Clear(); Assert.Throws<InvalidDataException>(() => store.Read(keys));
        a.Values[key] = data with { Generation = 99 }; Assert.Throws<InvalidDataException>(() => store.Read(keys));
        a.Values[key] = data with { Entries = new[] { "different" } }; Assert.Throws<InvalidDataException>(() => store.Read(keys));
        foreach (var pointer in new[] { Array.Empty<string>(), new[] { "other", "east", key, "1" }, new[] { "blob1", "absent", key, "1" }, new[] { "blob1", "east", key, "01" } })
        { catalog.Values["a"] = catalog.Values["a"] with { Entries = pointer }; Assert.Throws<InvalidDataException>(() => store.Read(keys)); }
        Assert.Throws<ArgumentException>(() => store.Read(Array.Empty<string>()));
        Assert.Throws<ArgumentException>(() => store.CompareExchange(keys, Array.Empty<JournalVersion>(), new IReadOnlyList<string>?[] { null }));
        Assert.Throws<ArgumentException>(() => new PartitionedJournalStore(catalog, new Dictionary<string, IAtomicJournalStore>(), _ => "east"));
        var wrong = new PartitionedJournalStore(catalog, new Dictionary<string, IAtomicJournalStore> { ["east"] = a }, _ => "absent");
        Assert.Throws<ArgumentException>(() => wrong.CompareExchange(keys, new[] { new JournalVersion(0, Array.Empty<string>()) }, new IReadOnlyList<string>?[] { Array.Empty<string>() }));
    }

    [Fact]
    public void DurabilityAndConfigurationAreValidatedBeforeStaging()
    {
        var (catalog, a, b, store) = Fixture();
        catalog.SupportsDurableFlush = false; Assert.False(store.SupportsDurableFlush);
        catalog.SupportsDurableFlush = true; b.SupportsDurableFlush = false; Assert.False(store.SupportsDurableFlush);
        b.SupportsDurableFlush = true; Assert.True(store.SupportsDurableFlush);
        var map = new Dictionary<string, IAtomicJournalStore> { ["east"] = a };
        Assert.Throws<ArgumentNullException>(() => new PartitionedJournalStore(null!, map, _ => "east"));
        Assert.Throws<ArgumentNullException>(() => new PartitionedJournalStore(catalog, null!, _ => "east"));
        Assert.Throws<ArgumentNullException>(() => new PartitionedJournalStore(catalog, map, null!));
        Assert.Throws<ArgumentNullException>(() => new PartitionedJournalStore(catalog, new Dictionary<string, IAtomicJournalStore> { ["east"] = null! }, _ => "east"));
        Assert.Throws<ArgumentException>(() => new PartitionedJournalStore(catalog, new Dictionary<string, IAtomicJournalStore> { ["../east"] = a }, _ => "east"));
        var keys = new[] { "a", "b" };
        var wrongRoute = new PartitionedJournalStore(catalog, map, key => key == "a" ? "east" : "missing");
        Assert.Throws<ArgumentException>(() => wrongRoute.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { new[] { "one" }, new[] { "two" } }));
        Assert.Empty(a.Values); Assert.Empty(catalog.Values);
        Assert.Throws<ArgumentException>(() => store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { null }));
    }

    [Fact]
    public void MalformedSnapshotsAndEveryPointerCoordinateFailClosed()
    {
        var (catalog, a, _, store) = Fixture(); var keys = new[] { "a" };
        Assert.True(store.CompareExchange(keys, store.Read(keys), new IReadOnlyList<string>?[] { new[] { "original" } }));
        var pointer = catalog.Values["a"]; var blob = pointer.Entries[2];
        foreach (var generation in new[] { "", "0", "-1", "+1", "1 ", "18446744073709551616", "01" })
        {
            catalog.Values["a"] = pointer with { Entries = new[] { "blob1", "east", blob, generation } };
            Assert.Throws<InvalidDataException>(() => store.Read(keys));
        }
        foreach (var address in new[] { "../bad", "blob1-000000000000000G", "blob1-0000000000000000\n" })
        {
            catalog.Values["a"] = pointer with { Entries = new[] { "blob1", "east", address, "1" } };
            Assert.Throws<InvalidDataException>(() => store.Read(keys));
        }
        catalog.Values["a"] = pointer with { Generation = 0 };
        Assert.Throws<InvalidDataException>(() => store.Read(keys));
        catalog.Values["a"] = pointer;
        catalog.ReadOverride = _ => Array.Empty<JournalVersion>();
        Assert.Throws<InvalidDataException>(() => store.Read(keys));
        catalog.ReadOverride = null;
        a.ReadOverride = _ => Array.Empty<JournalVersion>();
        Assert.Throws<InvalidDataException>(() => store.Read(keys));
        Assert.Throws<InvalidDataException>(() => store.CompareExchange(keys, new[] { pointer }, new IReadOnlyList<string>?[] { new[] { "other" } }));
    }

    [Theory]
    [InlineData("identical")]
    [InlineData("different-content")]
    [InlineData("different-length")]
    [InlineData("absent")]
    public void ConcurrentObjectCreationReusesOnlyTheCompleteSameJournal(string competing)
    {
        var (catalog, a, _, store) = Fixture(); var keys = new[] { "a" }; var snapshot = store.Read(keys);
        a.ExchangeOverride = (objectKeys, _, updates) =>
        {
            if (competing != "absent") a.Values[objectKeys[0]] = new(1, competing switch
            {
                "identical" => updates[0]!.ToArray(),
                "different-content" => new[] { "collision" },
                _ => Array.Empty<string>()
            });
            return false;
        };
        if (competing == "identical")
        {
            Assert.True(store.CompareExchange(keys, snapshot, new IReadOnlyList<string>?[] { new[] { "original" } }));
            Assert.Equal(new[] { "original" }, store.Read(keys)[0].Entries);
        }
        else
        {
            Assert.Throws<InvalidDataException>(() => store.CompareExchange(keys, snapshot, new IReadOnlyList<string>?[] { new[] { "original" } }));
            Assert.Empty(catalog.Values);
        }
    }
}
