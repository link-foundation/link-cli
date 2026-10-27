using Foundation.Data.Doublets.Cli.Engine;
using Microsoft.Win32.SafeHandles;

namespace Foundation.Data.Doublets.Cli.Tests;

public class NativeLsmReadOnlyContractsTests
{
    private sealed class Io : FileLsmIo, IDisposable
    {
        internal int FailOpenAt, Opens;
        internal bool FailClose;
        internal readonly IOException OpenFailure = new("discovery open failed");
        internal readonly List<FileStream> Files = new();
        internal readonly List<SafeFileHandle> Handles = new();
        internal readonly List<string> Closed = new();
        internal override FileStream OpenReader(string path)
        {
            if (++Opens == FailOpenAt) throw OpenFailure;
            var file = base.OpenReader(path); Files.Add(file); Handles.Add(file.SafeFileHandle); return file;
        }
        internal override void Close(FileStream file)
        {
            Closed.Add(Path.GetFileName(file.Name)); base.Close(file);
            if (FailClose) throw new IOException("discovery close failed: " + Path.GetFileName(file.Name));
        }
        public void Dispose() { foreach (var file in Files) file.Dispose(); }
    }
    private static void Run(Action<string, Io> action)
    {
        string directory = Directory.CreateTempSubdirectory("discovery-guards-").FullName;
        try { using var io = new Io(); action(Path.Combine(directory, "table"), io); }
        finally { Directory.Delete(directory, recursive: true); }
    }
    private static void Seed(string path)
    {
        File.WriteAllBytes(path, Enumerable.Range(0, 4096).Select(i => (byte)(i % 251)).ToArray());
        File.WriteAllBytes(path + ".meta0", Enumerable.Repeat((byte)7, 128).ToArray());
        File.WriteAllBytes(path + ".meta1", Array.Empty<byte>());
    }
    private static byte[][] Image(string path) => FileLsmFiles.Parts(path).Select(File.ReadAllBytes).ToArray();
    private static void DenyEffects(IPageDevice device)
    {
        Action[] effects = {
            () => device.NewDatabaseId(), () => device.WriteMetadata(0, new byte[128]),
            () => device.SyncMetadata(0), () => device.WritePage(0, new byte[4096]),
            device.SyncData, () => device.TruncateData(0)
        };
        foreach (var effect in effects) Assert.Contains("Read-only native LSM", Assert.Throws<InvalidOperationException>(effect).Message);
    }

    [Fact]
    public void PhysicalDiscoveryDeniesEveryEffectAndReturnsOwnedReads() => Run((path, io) =>
    {
        Seed(path); var before = Image(path);
        using var reader = new NativeLsmReadOnly(path, io);
        Assert.Equal(4096, reader.DataLength); Assert.False(((IPageDevice)reader).SupportsDurableFlush);
        var page = reader.ReadPage(0); Assert.Equal(before[0], page); page[0] ^= 1;
        Assert.Equal(before[0], reader.ReadPage(0));
        var metadata = reader.ReadMetadata(0)!; Assert.Equal(before[1], metadata); metadata[0] ^= 1;
        Assert.Equal(before[1], reader.ReadMetadata(0)); Assert.Null(reader.ReadMetadata(1));
        int opens = io.Opens;
        DenyEffects(reader);
        Assert.Throws<InvalidOperationException>(() => reader.RepairPage(0, new byte[4096]));
        Assert.Equal(opens, io.Opens); Assert.Empty(io.Closed); Assert.Equal(before, Image(path));
        reader.Dispose(); reader.Dispose();
        Assert.Equal(new[] { "table.meta1", "table.meta0", "table" }, io.Closed);
        Assert.All(io.Handles, handle => Assert.True(handle.IsClosed));
        Assert.Throws<ObjectDisposedException>(() => reader.DataLength);
        Assert.Throws<ObjectDisposedException>(() => reader.ReadMetadata(0));
        Assert.Throws<ObjectDisposedException>(() => reader.ReadPage(0));
        Assert.Equal(before, Image(path));
    });

    [Fact]
    public void MissingMalformedAndOutOfRangeReadsNeverCreateOrRepairParts() => Run((path, io) =>
    {
        using (var empty = new NativeLsmReadOnly(path, io))
        {
            Assert.Equal(0, empty.DataLength); Assert.Null(empty.ReadMetadata(0)); Assert.Null(empty.ReadMetadata(1));
            Assert.Throws<EndOfStreamException>(() => empty.ReadPage(0));
            Assert.Empty(Directory.GetFileSystemEntries(Path.GetDirectoryName(path)!)); Assert.Equal(0, io.Opens);
        }
        File.WriteAllBytes(path + ".meta0", new byte[127]); File.WriteAllBytes(path + ".meta1", Array.Empty<byte>());
        using var malformed = new NativeLsmReadOnly(path, io);
        Assert.Equal(new byte[1], malformed.ReadMetadata(0)); Assert.Null(malformed.ReadMetadata(1));
        foreach (int slot in new[] { -1, 2 }) Assert.Throws<ArgumentOutOfRangeException>(() => malformed.ReadMetadata(slot));
        Assert.Throws<ArgumentOutOfRangeException>(() => malformed.ReadPage(PageFormat.MaxPages));
        Assert.Throws<EndOfStreamException>(() => malformed.ReadPage(0));
        Assert.False(File.Exists(path)); Assert.Equal(new byte[127], File.ReadAllBytes(path + ".meta0"));
    });

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void FailedAcquisitionClosesPriorHandlesAndPreservesBothFailures(bool failClose) => Run((path, io) =>
    {
        Seed(path); var before = Image(path); io.FailOpenAt = 2; io.FailClose = failClose;
        var error = Assert.ThrowsAny<Exception>(() => new NativeLsmReadOnly(path, io));
        if (failClose)
        {
            var causes = Assert.IsType<AggregateException>(error).Flatten().InnerExceptions;
            Assert.Contains(io.OpenFailure, causes); Assert.Contains(causes, cause => cause.Message.Contains("discovery close failed", StringComparison.Ordinal));
        }
        else Assert.Same(io.OpenFailure, error);
        Assert.Equal(new[] { "table" }, io.Closed); Assert.Single(io.Files);
        Assert.All(io.Handles, handle => Assert.True(handle.IsClosed)); Assert.Equal(before, Image(path));
    });

    [Fact]
    public void FailedCleanupAttemptsAllHandlesOnceAndCachesTheSameFailure() => Run((path, io) =>
    {
        Seed(path); var before = Image(path); var reader = new NativeLsmReadOnly(path, io); io.FailClose = true;
        var first = Assert.Throws<AggregateException>(reader.Dispose);
        Assert.Equal(3, first.Flatten().InnerExceptions.Count);
        Assert.Same(first, Assert.Throws<AggregateException>(reader.Dispose));
        Assert.Equal(new[] { "table.meta1", "table.meta0", "table" }, io.Closed);
        Assert.All(io.Handles, handle => Assert.True(handle.IsClosed));
        Assert.Throws<ObjectDisposedException>(() => reader.ReadPage(0)); Assert.Equal(before, Image(path));
    });

    [Fact]
    public void DeferredTableRetainsReadCapabilitiesWithoutForwardingEffectsOrRecovery()
    {
        var fixture = PagedTestDevice.Fixture("oneGroup").Image;
        foreach (bool durable in new[] { false, true }) foreach (uint flags in new uint[] { 0, 1 })
        {
            var device = PagedTestDevice.From(fixture); device.SupportsDurableFlush = durable; device.MetadataFlags = flags;
            IPageDevice deferred = new NativeLsmDeferred(device);
            Assert.Equal(durable, deferred.SupportsDurableFlush); Assert.Equal(flags, deferred.MetadataFlags);
            Assert.Equal(fixture.Pages.Count * 4096L, deferred.DataLength);
            Assert.Equal(fixture.Metadata[0], deferred.ReadMetadata(0));
            Assert.Equal(fixture.Pages[0], deferred.ReadPage(0)); Assert.Equal(fixture.Pages[0], deferred.ReadCheckedPage(0, 0));
            string[] calls = device.Calls.ToArray(); int reads = device.Reads;
            DenyEffects(deferred); deferred.CompleteRecovery();
            Assert.Equal(calls, device.Calls); Assert.Equal(reads, device.Reads); Assert.Equal(0, device.Completed);
            PagedTestDevice.Same(fixture, device.Live); PagedTestDevice.Same(fixture, device.Durable);
        }
    }

    [Fact]
    public void BorrowedWalOwnsEachReadAndRejectsAppendFlushAndReplacement()
    {
        string[] original = { "acknowledged\0😀", "" };
        IReplaceableCommitLog wal = new NativeLsmReadOnlyWal(original);
        Assert.False(wal.SupportsDurableFlush); Assert.True(wal.SupportsReplacement);
        Assert.Equal(original, wal.Read()); ((IList<string>)wal.Read())[0] = "caller edit";
        Assert.Equal(original, wal.Read());
        Assert.Throws<InvalidOperationException>(() => wal.Append("unselected"));
        Assert.Throws<InvalidOperationException>(wal.Flush);
        Assert.Throws<InvalidOperationException>(() => wal.Replace(new[] { "unselected" }));
        Assert.Equal(new[] { "acknowledged\0😀", "" }, wal.Read()); Assert.Equal(new[] { "acknowledged\0😀", "" }, original);
    }

    [Fact]
    public void InvalidParentsAreErrorsRatherThanMissingParts() => Run((path, io) =>
    {
        File.WriteAllText(path, "preserve parent");
        string child = Path.Combine(path, "child");
        foreach (bool missing in new[] { false, true })
            Assert.ThrowsAny<IOException>(() => NativeLsmFileType.Check(child, missing: missing));
        Assert.ThrowsAny<IOException>(() => new NativeLsmReadOnly(child, io));
        Assert.Equal(0, io.Opens); Assert.Equal("preserve parent", File.ReadAllText(path));
        Assert.Throws<InvalidDataException>(() => NativeLsmFileType.Check(path, directory: true));
    });

    [Fact]
    public void NonregularHandlesRejectWithoutReadingSpecialFiles()
    {
        // These OS null devices open immediately and need no data read. Never
        // open a FIFO or socket that could block a test before admission.
        using var file = new FileStream(OperatingSystem.IsWindows() ? "NUL" : "/dev/null", FileMode.Open, FileAccess.Read, FileShare.ReadWrite);
        Assert.Throws<InvalidDataException>(() => NativeLsmFileType.CheckHandle(file));
    }
}
