using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class PhysicalLsmMirrorTests
{
    private sealed class Namespace : ILsmTableNamespace
    {
        internal readonly SortedDictionary<ulong, PagedTestDevice[]> Files = new();
        internal readonly HashSet<IPageDevice> Leases = new();
        public bool SupportsDurableFlush => true;
        public IReadOnlyList<ulong> List(int maxEntries) { Assert.True(Files.Count <= maxEntries); return Files.Keys.ToArray(); }
        public IPageDevice Create(ulong id) { Files.Add(id, new[] { new PagedTestDevice(), new PagedTestDevice() }); return Open(id); }
        public IPageDevice Open(ulong id) { var pair = Files[id]; var device = new MirroredPageDevice(pair[0], pair[1]); Assert.True(Leases.Add(device)); return device; }
        public void Close(IPageDevice device) { Leases.Remove(device); }
        public void Delete(ulong id) { Assert.True(Files.Remove(id)); }
    }
    private static void Put(PhysicalLsmJournalStore store, string key) => Assert.True(store.CompareExchange(new[] { key }, store.Read(new[] { key }), new IReadOnlyList<string>?[] { new[] { key } }));
    [Fact]
    public void IndependentMirroredTablePagesPreservePointReadsAndRepairSurvivingCopy()
    {
        var h = new PhysicalLsmHarness(); var p = new Namespace(); var wal = h.Wal();
        using (var store = PhysicalLsmJournalStore.Open(wal, p, PhysicalLsmHarness.Options())) { Put(store, "a"); store.Flush(); Assert.Equal(new[] { "a" }, store.Read(new[] { "a" })[0].Entries); }
        Assert.Empty(p.Leases); var pair = p.Files[1]; pair[0].Live.Pages[0][0] ^= 1;
        using var reopened = PhysicalLsmJournalStore.Open(wal, p, PhysicalLsmHarness.Options()); Assert.Equal(new[] { "a" }, reopened.Read(new[] { "a" })[0].Entries); PagedTestDevice.Same(pair[1].Live, pair[0].Live);
    }
    [Fact]
    public void LaterBadBindingPreventsStagedMirroredRepairInEarlierSelectedTable()
    {
        var h = new PhysicalLsmHarness(); var p = new Namespace(); var wal = h.Wal(); var options = PhysicalLsmHarness.Options();
        using (var store = PhysicalLsmJournalStore.Open(wal, p, options)) { foreach (string key in new[] { "a", "b" }) { Put(store, key); store.Flush(); } }
        var first = p.Files[1]; first[0].Live.Pages[0][0] ^= 1; var before = first[0].Live.Copy();
        var state = LsmWal.Recover(wal.Read(), options.Limits); var manifest = state.Manifest.ToArray(); manifest[1] = manifest[1] with { Digest = manifest[1].Digest ^ 1 };
        wal.Replace(new[] { LsmWal.Encode(state with { Manifest = manifest }, options.Limits) });
        Assert.Contains("binding changed", Assert.Throws<InvalidDataException>(() => PhysicalLsmJournalStore.Open(wal, p, options)).Message);
        PagedTestDevice.Same(before, first[0].Live); Assert.Empty(p.Leases);
    }
}
