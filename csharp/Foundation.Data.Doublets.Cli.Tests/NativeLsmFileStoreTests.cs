using System.Security.Cryptography;
using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class NativeLsmFileStoreTests
{
    private static FileLsmOptions Options(int mode = 0) => new(PhysicalLsmHarness.Options(), 128, (mode & 1) != 0, (mode & 2) != 0, (mode & 4) != 0);
    private static void Run(Action<string> action)
    { string directory = Directory.CreateTempSubdirectory("native-lsm-").FullName; try { action(directory); } finally { Directory.Delete(directory, true); } }
    private static void Put(FileLsmJournalStore store, string key, string[] entries) => Assert.True(store.CompareExchange(new[] { key }, store.Read(new[] { key }), new IReadOnlyList<string>?[] { entries }));
    private static string[] DataFiles(string directory) => Directory.GetFiles(directory).Where(path => !path.EndsWith(".writer", StringComparison.Ordinal) && Path.GetFileName(path) != ".lsm-writer").Order().ToArray();
    private static string[] Image(string directory) => DataFiles(directory).Select(path => Path.GetFileName(path) + ":" + Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(path)))).ToArray();
    private static void OwnerFree(string directory) => Assert.DoesNotContain(Directory.GetFiles(directory), path => path.EndsWith(".writer", StringComparison.Ordinal) || Path.GetFileName(path) == ".lsm-writer");
    private static void Seed(string directory, int mode = 0)
    {
        using var store = FileLsmJournalStore.Open(directory, Options(mode));
        Put(store, "a", new[] { "old\0雪😀" }); Put(store, "b", Array.Empty<string>()); store.Flush();
        Put(store, "a", new[] { "selected" }); store.Flush(); Put(store, "c", new[] { "buffer" });
    }
    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(2)]
    [InlineData(3)]
    [InlineData(4)]
    [InlineData(5)]
    [InlineData(6)]
    [InlineData(7)]
    public void EveryMirrorCombinationPreservesBytesOwnershipAndFollowingCompaction(int mode) => Run(directory =>
    {
        Seed(directory, mode); string[] before = Image(directory);
        using (var store = FileLsmJournalStore.Open(directory, Options(mode)))
        {
            Assert.Equal(!OperatingSystem.IsWindows(), store.SupportsDurableFlush);
            var values = store.Read(new[] { "a", "b", "c" }); Assert.Equal(new[] { "selected" }, values[0].Entries); Assert.Empty(values[1].Entries);
            Assert.Equal("buffer", values[2].Entries[0]); ((string[])values[0].Entries)[0] = "caller";
            Assert.Equal("selected", store.Read(new[] { "a" })[0].Entries[0]);
            Assert.ThrowsAny<Exception>(() => FileLsmJournalStore.Open(directory, Options(mode)));
            Assert.False(store.CompareExchange(new[] { "a" }, new[] { new JournalVersion(0, Array.Empty<string>()) }, new IReadOnlyList<string>?[] { new[] { "lost" } }));
        }
        OwnerFree(directory); Assert.Equal(before, Image(directory));
        using (var store = FileLsmJournalStore.Open(directory, Options(mode)))
        { store.Compact(); Put(store, "d", new[] { "following" }); store.Flush(); }
        using (var reader = FileLsmJournalStore.Open(directory, Options(mode)))
        { Assert.Equal(new[] { "a", "b", "c", "d" }, reader.ScanNamespace().Select(row => row.Key)); Assert.Equal("following", reader.Read(new[] { "d" })[0].Entries[0]); }
        OwnerFree(directory);
    });
    [Fact]
    public void InvalidOptionsPrecedeAnyOwnershipIo() => Run(directory =>
    {
        foreach (var options in new[] { Options() with { MaxPages = 0 }, Options() with { MaxPages = 65537 }, Options() with { Store = new(MaxNamespaceEntries: 1) }, Options() with { Store = new(new LsmOptions(BufferKeys: 0)) } })
        { Assert.ThrowsAny<Exception>(() => FileLsmJournalStore.Open(directory, options)); Assert.Empty(Directory.GetFileSystemEntries(directory)); }
    });
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void MissingOrCorruptSelectedDataPreservesEveryOtherByte(bool missing) => Run(directory =>
    {
        Seed(directory); string table = DataFiles(directory).First(path => System.Text.RegularExpressions.Regex.IsMatch(Path.GetFileName(path), "^table-[0-9]+$"));
        if (missing) File.Delete(table); else { byte[] bytes = File.ReadAllBytes(table); bytes[100] ^= 1; File.WriteAllBytes(table, bytes); }
        foreach (string bank in new[] { "wal.bank0", "wal.bank1" }) { using var suffix = new FileStream(Path.Combine(directory, bank), FileMode.Append); suffix.Write(new byte[4096]); }
        string[] before = Image(directory); Assert.ThrowsAny<Exception>(() => FileLsmJournalStore.Open(directory, Options())); Assert.Equal(before, Image(directory)); OwnerFree(directory);
    });
    [Fact]
    public void ForeignNamesStaleOwnersAndUnrootedTablesAreNeverRemoved() => Run(directory =>
    {
        foreach (string name in new[] { "foreign", "table-01", "table-0", "table-18446744073709551616", "table-999", "wal.bank0.writer", "table-1.mirror" })
        {
            Seed(directory); string path = Path.Combine(directory, name); File.WriteAllText(path, "evidence"); string[] before = Image(directory);
            Assert.ThrowsAny<Exception>(() => FileLsmJournalStore.Open(directory, Options())); Assert.Equal(before, Image(directory)); Assert.True(File.Exists(path)); Assert.False(File.Exists(Path.Combine(directory, ".lsm-writer"))); File.Delete(path);
        }
        string stale = Path.Combine(directory, ".lsm-writer"); File.WriteAllText(stale, "old owner"); Assert.ThrowsAny<Exception>(() => FileLsmJournalStore.Open(directory, Options())); Assert.Equal("old owner", File.ReadAllText(stale));
    });
    [Fact]
    public void CompletelyUninitializedNamespaceCannotContainTables() => Run(directory =>
    {
        File.WriteAllText(Path.Combine(directory, "table-1"), "orphan"); string[] before = Image(directory);
        Assert.ThrowsAny<Exception>(() => FileLsmJournalStore.Open(directory, Options())); Assert.Equal(before, Image(directory)); OwnerFree(directory);
    });
}
