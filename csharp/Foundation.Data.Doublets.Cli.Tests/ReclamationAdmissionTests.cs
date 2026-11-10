using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public partial class ReclamationTests
{
    private static (ReclamationMemoryStore Catalog, ReclamationMemoryStore A, ReclamationMemoryStore B, ReclaimingPartitionedJournalStore Store) AdmissionLayout(bool chunked)
    {
        var c = new ReclamationMemoryStore(); var a = new ReclamationMemoryStore(); var b = new ReclamationMemoryStore();
        var store = ReclaimingPartitionedJournalStore.Open(c, new Dictionary<string, IJournalMaintenanceStore> { ["east"] = a, ["west"] = b }, key => key == "a" ? "east" : "west", null, chunked ? new JournalChunkOptions(MaxRecords: 2, MaxBytes: 64, MaxChunks: 32) : null);
        return (c, a, b, store);
    }
    private static void AssertAdmissionVersions(IReadOnlyList<JournalVersion> expected, IReadOnlyList<JournalVersion> actual)
    {
        Assert.Equal(expected.Count, actual.Count);
        for (var index = 0; index < expected.Count; index++)
        { Assert.Equal(expected[index].Generation, actual[index].Generation); Assert.Equal(expected[index].Entries, actual[index].Entries); }
    }
    [Fact(Timeout = 10000)]
    public async Task MismatchedArraysRejectBeforeIoAndPreserveFollowingWrites()
    {
        await Task.Yield(); foreach (var chunked in new[] { false, true })
        {
            var (c, a, b, store) = AdmissionLayout(chunked); var expected = Write(store, "old-a", "old-b"); var before = (c.Calls.Count, a.Calls.Count, b.Calls.Count);
            foreach (var (guards, updates) in new[] { (1, 2), (3, 2), (2, 1), (2, 3) })
            {
                Assert.Throws<ArgumentException>(() => store.CompareExchange(Keys, Enumerable.Repeat(expected[0], guards).ToArray(), Enumerable.Repeat<IReadOnlyList<string>?>(new[] { "must-not-stage" }, updates).ToArray()));
                Assert.Equal(before, (c.Calls.Count, a.Calls.Count, b.Calls.Count));
            }
            AssertAdmissionVersions(expected, store.Read(Keys)); Write(store, "following-a", "following-b");
        }
    }
    [Fact(Timeout = 10000)]
    public async Task GenerationZeroWithPayloadNeverBecomesAnAbsentValue()
    {
        await Task.Yield(); foreach (var chunked in new[] { false, true }) foreach (var phase in new[] { "shard", "epoch", "object" })
        {
            var (c, a, b, store) = AdmissionLayout(chunked); var expected = Write(store, "live-a", "live-b");
            var provider = phase == "object" ? a : c; var key = phase == "object" ? a.Values.Keys.Single() : phase == "epoch" ? EpochKey : "a"; var saved = provider.Values[key];
            provider.Values[key] = saved with { Generation = 0 }; var reads = a.Count("read"); var swaps = c.Count("swap");
            Assert.Throws<InvalidDataException>(() => store.Read(Keys)); Assert.Equal(phase == "object" ? 1 : 0, a.Count("read") - reads);
            Assert.Equal(swaps, c.Count("swap")); Assert.Equal(0, Deletions(a, b)); provider.Values[key] = saved;
            AssertAdmissionVersions(expected, store.Read(Keys)); Write(store, "following-a", "following-b");
        }
    }
    [Fact(Timeout = 10000)]
    public async Task NoncanonicalObjectKeysRejectBeforeDownloadOrDeletion()
    {
        await Task.Yield(); foreach (var chunked in new[] { false, true }) foreach (var position in new[] { 6, 21, 23, 38 })
        {
            var (c, a, b, store) = AdmissionLayout(chunked); Write(store, "orphan-a", "orphan-b");
            var expected = Write(store, "live-a", "live-b"); var original = c.Values["a"]; var keyIndex = chunked ? 4 : 2;
            var malformed = original.Entries.ToArray(); var badKey = malformed[keyIndex][..position] + "F" + malformed[keyIndex][(position + 1)..]; malformed[keyIndex] = badKey;
            c.Values["a"] = original with { Entries = malformed }; var reads = a.Count("read") + b.Count("read");
            Assert.Throws<InvalidDataException>(() => store.Read(Keys)); Assert.Equal(reads, a.Count("read") + b.Count("read"));
            Assert.Throws<InvalidDataException>(() => store.CollectGarbage()); Assert.Equal(0, Deletions(a, b)); c.Values["a"] = original;
            AssertAdmissionVersions(expected, store.Read(Keys));
            a.Values[badKey] = a.Values[original.Entries[keyIndex]];
            Assert.Throws<InvalidDataException>(() => store.CollectGarbage()); Assert.Equal(0, Deletions(a, b)); a.Values.Remove(badKey);
            Write(store, "following-a", "following-b");
        }
    }
    [Fact(Timeout = 10000)]
    public async Task UnchangedPointerCannotTurnAMissingObjectIntoARefresh()
    {
        await Task.Yield(); foreach (var chunked in new[] { false, true })
        {
            var (c, a, b, store) = AdmissionLayout(chunked); var expected = Write(store, "live-a", "live-b"); var original = c.Values["a"]; var objectKey = a.Values.Keys.Single(); var saved = a.Values[objectKey];
            a.On("read", () => { Assert.True(c.CompareExchange(new[] { "a" }, new[] { original }, new IReadOnlyList<string>?[] { original.Entries })); a.Values.Remove(objectKey); });
            var reads = (c.Count("read"), a.Count("read"), b.Count("read"));
            Assert.Throws<InvalidDataException>(() => store.Read(Keys)); Assert.Equal((reads.Item1 + 2, reads.Item2 + 1, reads.Item3), (c.Count("read"), a.Count("read"), b.Count("read")));
            a.Values[objectKey] = saved; var fresh = store.Read(Keys); Assert.Equal(expected[0].Entries, fresh[0].Entries); Assert.NotEqual(expected[0].Generation, fresh[0].Generation);
            Write(store, "following-a", "following-b");
        }
    }
}
