using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class PhysicalLsmRecoveryTests
{
    private static PhysicalLsmHarness.Snapshot Sealed()
    {
        var h = new PhysicalLsmHarness(); using var s = h.Open();
        foreach (var key in new[] { "a", "b" }) { Assert.True(s.CompareExchange(new[] { key }, s.Read(new[] { key }), new IReadOnlyList<string>?[] { new[] { key } })); s.Flush(); }
        return h.Image();
    }
    [Fact]
    public void AllSelectedBindingsValidateBeforeAnyRepairCompletes()
    {
        var image = Sealed(); var broken = new PhysicalLsmHarness(image); var wal = broken.Wal(); var limits = PhysicalLsmHarness.Options().Limits;
        var state = LsmWal.Recover(wal.Read(), limits); var manifest = state.Manifest.ToArray(); manifest[1] = manifest[1] with { Digest = manifest[1].Digest ^ 1 };
        wal.Replace(new[] { LsmWal.Encode(state with { Manifest = manifest }, limits) }); var before = broken.Image(); int repairs = 0;
        foreach (var device in broken.Tables.Files.Values) device.OnComplete = () => repairs++;
        broken.Reset(); Assert.Contains("binding changed", Assert.Throws<InvalidDataException>(() => PhysicalLsmJournalStore.Open(wal, broken.Tables, PhysicalLsmHarness.Options())).Message);
        Assert.Equal(0, repairs); PhysicalLsmHarness.Same(before, broken.Image()); Assert.Empty(broken.Tables.Leases);
        var good = new PhysicalLsmHarness(image); int complete = 0;
        foreach (var device in good.Tables.Files.Values) device.OnComplete = () => { Assert.Equal(2, good.Tables.Leases.Count); complete++; };
        using var s = good.Open(); Assert.Equal(2, complete); Assert.Equal(new[] { "a", "b" }, s.ScanNamespace().Select(row => row.Key));
    }
    [Fact]
    public void CloseFailureRetainsOwnershipForExplicitRetry()
    {
        var h = new PhysicalLsmHarness(Sealed()); var s = h.Open(); h.Reset(); h.Fault = (1, false);
        Assert.Single(Assert.Throws<AggregateException>(s.Dispose).InnerExceptions); Assert.Single(h.Tables.Leases);
        Assert.Throws<ObjectDisposedException>(() => s.Read(new[] { "a" })); h.Fault = null; s.Dispose(); Assert.Empty(h.Tables.Leases); s.Dispose();
    }
    [Fact]
    public void DurableNamespaceCannotSilentlyDowngradeSelectedDevice()
    {
        var h = new PhysicalLsmHarness(Sealed()); h.Tables.Files.First().Value.Inner.SupportsDurableFlush = false; var before = h.Image(); h.Reset();
        Assert.Contains("non-durable", Assert.Throws<InvalidDataException>(() => h.Open()).Message); Assert.Empty(h.Tables.Leases);
        PhysicalLsmHarness.Same(before, h.Image()); Assert.All(h.Trace, name => Assert.StartsWith("close:", name));
    }
}
