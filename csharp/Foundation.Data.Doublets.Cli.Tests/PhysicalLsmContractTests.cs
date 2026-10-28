using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class PhysicalLsmContractTests
{
    private static void Put(PhysicalLsmJournalStore store, string key, string[] value) => Assert.True(store.CompareExchange(new[] { key }, store.Read(new[] { key }), new IReadOnlyList<string>?[] { value }));
    [Fact]
    public void InvalidGroupsAndBoundedOwnedScansPreserveTheDurableImage()
    {
        var h = new PhysicalLsmHarness(); using var s = h.Open(); h.Reset(); var before = h.Image();
        foreach (var keys in new[] { Array.Empty<string>(), Enumerable.Repeat("a", 33).ToArray(), new[] { "a", "a" }, new[] { "" }, new[] { "雪" }, new[] { new string('a', 129) } })
            Assert.ThrowsAny<ArgumentException>(() => s.Read(keys));
        Assert.Throws<ArgumentException>(() => s.CompareExchange(new[] { "a" }, Array.Empty<JournalVersion>(), Array.Empty<IReadOnlyList<string>?>()));
        Assert.Throws<ArgumentNullException>(() => Put(s, "a", new string[] { null! }));
        Assert.Throws<ArgumentOutOfRangeException>(() => s.CompareDelete("a", 0));
        Assert.False(s.CompareDelete("missing", 1)); Assert.Empty(h.Trace); PhysicalLsmHarness.Same(before, h.Image());
        Put(s, "a", new[] { "A" }); Put(s, "b", Array.Empty<string>()); s.Flush();
        Assert.All(s.ScanNamespace(new(KeysOnly: true)), row => Assert.Empty(row.Version.Entries));
        Assert.Throws<InvalidDataException>(() => s.ScanNamespace(new(MaxKeys: 1)));
        Assert.Throws<InvalidDataException>(() => s.ScanNamespace(new(MaxBytes: 9)));
        var rows = s.ScanNamespace(); ((string[])rows[0].Version.Entries)[0] = "caller"; Assert.Equal(new[] { "A" }, s.Read(new[] { "a" })[0].Entries);
    }
    [Fact]
    public void GroupCapacityAndCompleteNamespaceAdmissionPrecedeEffects()
    {
        foreach (int mode in Enumerable.Range(0, 3))
        {
            var h = new PhysicalLsmHarness(); var options = PhysicalLsmHarness.Options();
            options = options with { Limits = mode switch { 0 => options.Limits! with { MaxWalRecords = 1 }, 1 => options.Limits! with { MaxKeys = 1, BufferKeys = 1 }, _ => options.Limits! with { MaxBytes = 16, BufferBytes = 16 } } };
            using var s = PhysicalLsmJournalStore.Open(h.Wal(), h.Tables, options); if (mode == 1) Put(s, "a", Array.Empty<string>()); h.Reset(); var before = h.Image();
            Assert.Throws<InvalidDataException>(() => Put(s, mode == 1 ? "b" : "a", mode == 2 ? new[] { new string('x', 8) } : Array.Empty<string>()));
            Assert.Empty(h.Trace); PhysicalLsmHarness.Same(before, h.Image());
        }
        var bounded = new PhysicalLsmHarness(); var selected = PhysicalLsmHarness.Options(); selected = selected with { MaxNamespaceEntries = 2, Limits = selected.Limits! with { MaxRuns = 4 } };
        using var store = PhysicalLsmJournalStore.Open(bounded.Wal(), bounded.Tables, selected);
        foreach (string key in new[] { "a", "b" }) { Put(store, key, Array.Empty<string>()); store.Flush(); }
        Put(store, "c", Array.Empty<string>()); bounded.Reset(); var image = bounded.Image();
        Assert.Contains("namespace", Assert.Throws<InvalidDataException>(store.Flush).Message); Assert.Empty(bounded.Trace); PhysicalLsmHarness.Same(image, bounded.Image());
    }
    [Fact]
    public void HotKeyReplayRotatesWithinRecordAndByteBoundsWithoutTablePublication()
    {
        foreach (bool bytes in new[] { false, true })
        {
            var h = new PhysicalLsmHarness(); var options = PhysicalLsmHarness.Options(); options = options with { Limits = options.Limits! with { BufferKeys = 32, MaxWalRecords = bytes ? 100 : 2, MaxWalBytes = bytes ? 180 : 8192 } };
            var wal = h.Wal(); using (var s = PhysicalLsmJournalStore.Open(wal, h.Tables, options)) { for (int i = 0; i < 12; i++) Put(s, "a", new[] { i.ToString(System.Globalization.CultureInfo.InvariantCulture) }); Assert.Empty(h.Tables.Files); Assert.True(wal.Read().Count < 12); }
            var fresh = new PhysicalLsmHarness(h.Image()); using var reopened = PhysicalLsmJournalStore.Open(fresh.Wal(), fresh.Tables, options); Assert.Equal(new[] { "11" }, reopened.Read(new[] { "a" })[0].Entries);
        }
    }
}
