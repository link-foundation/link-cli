using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class NativeLsmFileDiscoveryTests
{
    private static void Run(Action<string> action)
    { string directory = Directory.CreateTempSubdirectory("lsm-discovery-").FullName; try { action(directory); } finally { Directory.Delete(directory, true); } }
    [Fact]
    public void MissingImagesAreReadOnlyAndEveryMutationIsRejected() => Run(directory =>
    {
        string path = Path.Combine(directory, "missing"); using var device = new NativeLsmReadOnly(path, new());
        Assert.Equal(0, device.DataLength); Assert.Null(device.ReadMetadata(0)); Assert.Null(device.ReadMetadata(1)); Assert.Empty(Directory.GetFiles(directory));
        Assert.Throws<EndOfStreamException>(() => device.ReadPage(0)); Assert.Throws<ArgumentOutOfRangeException>(() => device.ReadPage(PageFormat.MaxPages));
        Assert.Throws<ArgumentOutOfRangeException>(() => device.ReadMetadata(2));
        Assert.Throws<InvalidOperationException>(() => device.NewDatabaseId()); Assert.Throws<InvalidOperationException>(() => device.WriteMetadata(0, new byte[128]));
        Assert.Throws<InvalidOperationException>(() => device.SyncMetadata(0)); Assert.Throws<InvalidOperationException>(() => device.WritePage(0, new byte[4096]));
        Assert.Throws<InvalidOperationException>(() => device.RepairPage(0, new byte[4096])); Assert.Throws<InvalidOperationException>(device.SyncData); Assert.Throws<InvalidOperationException>(() => device.TruncateData(0));
        device.Dispose(); device.Dispose(); Assert.Throws<ObjectDisposedException>(() => device.ReadMetadata(0));
    });
    [Fact]
    public void FixedMetadataAndPageReadsNeverAllocateFromMalformedLengths() => Run(directory =>
    {
        string path = Path.Combine(directory, "image"); File.WriteAllBytes(path, new byte[4096]); File.WriteAllBytes(path + ".meta0", new byte[128]); File.WriteAllBytes(path + ".meta1", new byte[129]);
        using var device = new NativeLsmReadOnly(path, new()); Assert.Equal(4096, device.DataLength); Assert.Equal(128, device.ReadMetadata(0)!.Length); Assert.Single(device.ReadMetadata(1)!);
        Assert.Equal(4096, device.ReadPage(0).Length); Assert.Throws<EndOfStreamException>(() => device.ReadPage(1));
    });
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void DiscoveryCloseUncertaintyAttemptsEveryHandleOnceAndKeepsOuterFence(bool after) => Run(directory =>
    {
        var options = new FileLsmOptions(MaxPages: 128); using (var seed = FileLsmJournalStore.Open(directory, options)) { }
        var io = new NativeLsmFileHarness();
        try
        {
            io.Reset(); using (var baseline = FileLsmJournalStore.Open(directory, options, io)) { }
            int firstClose = io.Trace.FindIndex(operation => operation.StartsWith("close:", StringComparison.Ordinal)) + 1; Assert.True(firstClose > 1);
            io.Reset((firstClose, after)); Assert.ThrowsAny<Exception>(() => FileLsmJournalStore.Open(directory, options, io)); Assert.True(io.Hit); io.Active = false;
            Assert.Equal(after ? 0 : 1, io.Live.Count); Assert.True(File.Exists(Path.Combine(directory, ".lsm-writer")));
            Assert.Throws<IOException>(() => FileLsmJournalStore.Open(directory, options));
        }
        finally { io.Recover(directory); }
        using var reopened = FileLsmJournalStore.Open(directory, options); Assert.Empty(reopened.ScanNamespace());
    });
    [Fact]
    public void MirroredRepairWaitsForLaterSelectedBindingBeforeAnyPhysicalChange() => Run(directory =>
    {
        var options = new FileLsmOptions(PhysicalLsmHarness.Options(), 128, true, true, true);
        using (var seed = FileLsmJournalStore.Open(directory, options))
        {
            foreach (string key in new[] { "a", "b" }) Assert.True(seed.CompareExchange(new[] { key }, seed.Read(new[] { key }), new IReadOnlyList<string>?[] { new[] { key } })); seed.Flush();
            Assert.True(seed.CompareExchange(new[] { "a" }, seed.Read(new[] { "a" }), new IReadOnlyList<string>?[] { new[] { "new" } })); seed.Flush();
        }
        string[] tables = Directory.GetFiles(directory).Where(path => System.Text.RegularExpressions.Regex.IsMatch(Path.GetFileName(path), "^table-[0-9]+$")).Order().ToArray(); Assert.True(tables.Length >= 2);
        byte[] damaged = File.ReadAllBytes(tables[0]); damaged[100] ^= 1; File.WriteAllBytes(tables[0], damaged);
        string[] metadata = new[] { ".meta0", ".meta1", ".mirror.meta0", ".mirror.meta1" }.Select(suffix => tables[1] + suffix).ToArray();
        byte[][] originals = metadata.Select(File.ReadAllBytes).ToArray(); foreach (string path in metadata) File.WriteAllBytes(path, Enumerable.Repeat((byte)22, 128).ToArray());
        string[] files = Directory.GetFiles(directory).Order().ToArray(); byte[][] before = files.Select(File.ReadAllBytes).ToArray();
        Assert.ThrowsAny<Exception>(() => FileLsmJournalStore.Open(directory, options)); Assert.Equal(files, Directory.GetFiles(directory).Order());
        for (int i = 0; i < files.Length; i++) Assert.Equal(before[i], File.ReadAllBytes(files[i]));
        for (int i = 0; i < metadata.Length; i++) File.WriteAllBytes(metadata[i], originals[i]);
        using (var recovered = FileLsmJournalStore.Open(directory, options)) { Assert.Equal("new", recovered.Read(new[] { "a" })[0].Entries[0]); }
        Assert.Equal(File.ReadAllBytes(tables[0] + ".mirror"), File.ReadAllBytes(tables[0]));
    });
    [Fact]
    public void MissingRedundantImagesStayAbsentUntilEverySelectedTableValidates() => Run(directory =>
    {
        var options = new FileLsmOptions(PhysicalLsmHarness.Options(), 128, true, true, true);
        using (var seed = FileLsmJournalStore.Open(directory, options))
        {
            foreach (string key in new[] { "a", "b" }) Assert.True(seed.CompareExchange(new[] { key }, seed.Read(new[] { key }), new IReadOnlyList<string>?[] { new[] { key } })); seed.Flush();
            Assert.True(seed.CompareExchange(new[] { "a" }, seed.Read(new[] { "a" }), new IReadOnlyList<string>?[] { new[] { "new" } })); seed.Flush();
        }
        string[] tables = Directory.GetFiles(directory).Where(path => System.Text.RegularExpressions.Regex.IsMatch(Path.GetFileName(path), "^table-[0-9]+$")).Order().ToArray(); Assert.True(tables.Length >= 2);
        string[] missing = FileLsmFiles.Parts(tables[0]).Concat(new[] { Path.Combine(directory, "wal.meta0"), Path.Combine(directory, "wal.meta1") }).ToArray(); foreach (string path in missing) File.Delete(path);
        string[] damaged = new[] { tables[1], tables[1] + ".mirror" }; byte[][] originals = damaged.Select(File.ReadAllBytes).ToArray();
        foreach (string path in damaged) { byte[] bytes = File.ReadAllBytes(path); bytes[100] ^= 1; File.WriteAllBytes(path, bytes); }
        string[] files = Directory.GetFiles(directory).Order().ToArray(); byte[][] before = files.Select(File.ReadAllBytes).ToArray();
        Assert.ThrowsAny<Exception>(() => FileLsmJournalStore.Open(directory, options)); Assert.Equal(files, Directory.GetFiles(directory).Order());
        for (int i = 0; i < files.Length; i++) Assert.Equal(before[i], File.ReadAllBytes(files[i])); foreach (string path in missing) Assert.False(File.Exists(path));
        for (int i = 0; i < damaged.Length; i++) File.WriteAllBytes(damaged[i], originals[i]);
        using (var recovered = FileLsmJournalStore.Open(directory, options)) Assert.Equal("new", recovered.Read(new[] { "a" })[0].Entries[0]);
        foreach (string path in missing) Assert.True(File.Exists(path)); Assert.Equal(File.ReadAllBytes(tables[0] + ".mirror"), File.ReadAllBytes(tables[0]));
    });
    [Fact]
    public void APlainTableWithOneCompleteMetadataSlotRemainsReadable() => Run(directory =>
    {
        var options = new FileLsmOptions(PhysicalLsmHarness.Options(), 128);
        using (var seed = FileLsmJournalStore.Open(directory, options))
        { Assert.True(seed.CompareExchange(new[] { "a" }, seed.Read(new[] { "a" }), new IReadOnlyList<string>?[] { new[] { "saved" } })); seed.Flush(); }
        string table = Directory.GetFiles(directory).First(path => System.Text.RegularExpressions.Regex.IsMatch(Path.GetFileName(path), "^table-[0-9]+$"));
        var first = PageFormat.Metadata.Decode(File.ReadAllBytes(table + ".meta0"), 0); File.Delete(table + (first is { Pages: > 0 } ? ".meta1" : ".meta0"));
        using var reopened = FileLsmJournalStore.Open(directory, options); Assert.Equal("saved", reopened.Read(new[] { "a" })[0].Entries[0]);
    });
}
