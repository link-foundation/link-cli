using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class PhysicalLsmSelectionTests
{
    private static SortedJournalEntry Row(string key, ulong generation, string? value) => new(key, generation, value is null ? null : new[] { value });
    private static PhysicalLsmHarness Retained(SortedJournalEntry[][] rows, ulong generation, LsmStoreOptions options)
    {
        var h = new PhysicalLsmHarness(); var manifest = new List<LsmTableDescriptor>();
        foreach (var records in rows)
        {
            ulong id = (ulong)manifest.Count + 1; var device = h.Tables.Create(id); var table = SortedJournalTable.Create(device, records);
            manifest.Add(new(id, Convert.ToHexString(table.Identity).ToLowerInvariant(), table.Digest, table.Count, table.StoredBytes)); h.Tables.Close(device);
        }
        h.Wal().Replace(new[] { LsmWal.Encode(new(generation, generation, (ulong)manifest.Count, manifest, Array.Empty<SortedJournalEntry>()), options.Limits) }); return new PhysicalLsmHarness(h.Image());
    }
    [Fact]
    public void PhysicalGenerationsAndAllHistoricalDuplicatesValidateBeforeCleanup()
    {
        var options = new LsmStoreOptions(PhysicalLsmHarness.Options().Limits! with { MaxRuns = 4 }, 16);
        foreach (var (rows, generation, message) in new[]{
            (new[]{new[]{Row("a",2,null)}},1UL,"exceeds coverage"),
            (new[]{new[]{Row("a",1,null)}},2UL,"missing the global generation"),
            (new[]{new[]{Row("a",1,"old")},new[]{Row("a",2,"new")},new[]{Row("a",1,"conflict")}},2UL,"conflicting table versions")})
        {
            var h = Retained(rows, generation, options); var before = h.Image();
            Assert.Contains(message, Assert.Throws<InvalidDataException>(() => PhysicalLsmJournalStore.Open(h.Wal(), h.Tables, options)).Message);
            PhysicalLsmHarness.Same(before, h.Image()); Assert.Empty(h.Tables.Leases); Assert.All(h.Trace, name => Assert.StartsWith("close:", name));
            Assert.All(h.Tables.Files.Values, d => Assert.Equal(0, d.Inner.Completed));
        }
    }
    [Fact]
    public void CombinedRetainedKeyAndByteBoundsPrecedeCleanup()
    {
        foreach (bool bytes in new[] { false, true })
        {
            var options = PhysicalLsmHarness.Options(); options = options with { Limits = bytes ? options.Limits! with { MaxBytes = 20, BufferBytes = 20 } : options.Limits! with { MaxKeys = 1, BufferKeys = 1 } };
            var h = Retained(new[] { new[] { Row("a", 1, "A") }, new[] { Row("b", 2, "B") } }, 2, options); var before = h.Image();
            Assert.Contains("retained capacity", Assert.Throws<InvalidDataException>(() => PhysicalLsmJournalStore.Open(h.Wal(), h.Tables, options)).Message);
            PhysicalLsmHarness.Same(before, h.Image()); Assert.Empty(h.Tables.Leases);
        }
    }
    [Fact]
    public void GenerationExhaustionRejectsChangesWhileExactNoopsRemainPossible()
    {
        var options = PhysicalLsmHarness.Options(); var h = Retained(new[] { new[] { Row("a", ulong.MaxValue, "A") } }, ulong.MaxValue, options);
        using var s = h.Open(); var keys = new[] { "a" }; var before = s.Read(keys); h.Reset(); var image = h.Image();
        Assert.True(s.CompareExchange(keys, before, new IReadOnlyList<string>?[] { new[] { "A" } }));
        Assert.Contains("exhausted", Assert.Throws<InvalidDataException>(() => s.CompareExchange(keys, before, new IReadOnlyList<string>?[] { new[] { "changed" } })).Message);
        Assert.Empty(h.Trace); PhysicalLsmHarness.Same(image, h.Image());
    }
}
