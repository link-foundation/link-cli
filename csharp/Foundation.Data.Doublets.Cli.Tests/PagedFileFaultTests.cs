using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class PagedFileFaultTests
{
    private sealed class FaultFile(string path, bool owner) : FileStream(path,
        owner ? FileMode.CreateNew : FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.Read, bufferSize: 1)
    {
        internal bool FailClose, FailSync;
        internal int Closed, ReadLimit = int.MaxValue;
        internal int? PartialWrite;
        public override void Write(ReadOnlySpan<byte> bytes)
        {
            if (PartialWrite is { } count) { base.Write(bytes[..Math.Min(count, bytes.Length)]); throw new IOException("injected partial write"); }
            base.Write(bytes);
        }
        public override int Read(Span<byte> bytes) => base.Read(bytes[..Math.Min(ReadLimit, bytes.Length)]);
        public override void Flush(bool flushToDisk)
        {
            if (FailSync) throw new IOException("injected sync"); base.Flush(flushToDisk);
        }
        protected override void Dispose(bool disposing)
        {
            base.Dispose(disposing);
            if (disposing) { Closed++; if (FailClose) throw new IOException("injected close"); }
        }
    }
    private static void WithDirectory(Action<string> run)
    {
        string directory = Directory.CreateTempSubdirectory("page-file-fault-").FullName;
        try { run(Path.Combine(directory, "journal")); } finally { Directory.Delete(directory, recursive: true); }
    }
    [Fact]
    public void UncertainCloseAttemptsEveryHandleAndRetainsTheOwnerFile()
    {
        WithDirectory(path =>
        {
            var files = new List<FaultFile>(); var device = new FilePageDevice(path, (name, owner) =>
            { var file = new FaultFile(name, owner); files.Add(file); return file; }, _ => { });
            foreach (var file in files) file.FailClose = true;
            var error = Assert.Throws<AggregateException>(() => device.Dispose()); Assert.Equal(4, error.InnerExceptions.Count);
            Assert.All(files, file => Assert.Equal(1, file.Closed)); Assert.True(File.Exists(path + ".writer"));
            Assert.ThrowsAny<IOException>(() => new FilePageDevice(path)); device.Dispose();
            Assert.True(File.Exists(path + ".writer")); Assert.Throws<ObjectDisposedException>(() => device.SyncData());
        });
    }
    [Fact]
    public void ConstructorFailuresReleaseOnlyOwnedResourcesAndPreserveFiles()
    {
        WithDirectory(path =>
        {
            File.WriteAllText(path, "unrelated");
            Assert.Throws<IOException>(() =>
            {
                using var device = new FilePageDevice(path); _ = new PagedCommitLog(device);
            });
            Assert.Equal("unrelated", File.ReadAllText(path)); Assert.False(File.Exists(path + ".writer"));
            Assert.ThrowsAny<IOException>(() => new FilePageDevice(Path.Combine(path + "-missing", "journal")));
            if (!OperatingSystem.IsWindows())
            {
                Assert.Throws<IOException>(() => new FilePageDevice(path, (name, owner) => new FaultFile(name, owner), _ => throw new IOException("directory sync")));
                Assert.False(File.Exists(path + ".writer"));
            }
            Assert.Throws<IOException>(() => new FilePageDevice(path, (name, owner) =>
            {
                if (!owner) throw new IOException("open data"); return new FaultFile(name, true);
            }, _ => { }));
            Assert.False(File.Exists(path + ".writer"));
            var combined = Assert.Throws<AggregateException>(() => new FilePageDevice(path, (name, owner) =>
            {
                if (!owner) throw new IOException("open data"); return new FaultFile(name, true) { FailClose = true };
            }, _ => { }));
            Assert.Equal(2, combined.InnerExceptions.Count); Assert.True(File.Exists(path + ".writer"));
        });
    }
    [Theory]
    [InlineData(0)]
    [InlineData(17)]
    public void PartialDataWritesFenceAndReopenOnlyThePriorDecision(int written)
    {
        WithDirectory(path =>
        {
            FaultFile? data = null;
            using (var device = new FilePageDevice(path, (name, owner) =>
            {
                var file = new FaultFile(name, owner); if (name == path) data = file; return file;
            }, _ => { }))
            {
                var log = new PagedCommitLog(device); log.Append("committed"); log.Flush();
                data!.PartialWrite = written; log.Append(new string('x', 2100)); Assert.Throws<IOException>(() => log.Flush());
                Assert.Throws<IOException>(() => log.Read()); Assert.Throws<IOException>(() => log.Append("retry"));
                data.PartialWrite = null;
            }
            using var reopened = new FilePageDevice(path); var next = new PagedCommitLog(reopened); Assert.Equal(new[] { "committed" }, next.Read());
            next.Append("following"); next.Flush(); Assert.Equal(new[] { "committed", "following" }, next.Read());
        });
    }
    [Fact]
    public void ShortReadsMalformedLengthsAndBoundsHaveFiniteOutcomes()
    {
        WithDirectory(path =>
        {
            var streams = new List<FaultFile>();
            using var device = new FilePageDevice(path, (name, owner) =>
            { var file = new FaultFile(name, owner); streams.Add(file); return file; }, _ => { });
            var log = new PagedCommitLog(device); log.Append("\ufeff雪\0😀"); log.Flush();
            foreach (var stream in streams) stream.ReadLimit = 3;
            Assert.Equal(new[] { "\ufeff雪\0😀" }, new PagedCommitLog(device).Read());
            streams[1].ReadLimit = 0; Assert.Throws<EndOfStreamException>(() => log.Read()); streams[1].ReadLimit = 3;
            foreach (int slot in new[] { -1, 2 })
            {
                Assert.Throws<IOException>(() => device.ReadMetadata(slot)); Assert.Throws<IOException>(() => device.SyncMetadata(slot));
                Assert.Throws<IOException>(() => device.WriteMetadata(slot, new byte[128]));
            }
            Assert.Throws<IOException>(() => device.ReadPage(PagedCommitLog.MaximumPages)); Assert.Throws<IOException>(() => device.WritePage(PagedCommitLog.MaximumPages, new byte[4096]));
            Assert.Throws<IOException>(() => device.WriteMetadata(0, new byte[1])); Assert.Throws<IOException>(() => device.WritePage(0, new byte[1]));
            Assert.Throws<IOException>(() => device.TruncateData(-1)); Assert.Throws<IOException>(() => device.TruncateData(long.MaxValue));
            Assert.Throws<IOException>(() => device.WritePage(0, new byte[4096]));
            streams[2].SetLength(1); Assert.Single(device.ReadMetadata(0)!);
            streams[1].FailSync = true; Assert.Throws<IOException>(() => device.SyncData()); streams[1].FailSync = false;
            device.Dispose(); device.Dispose(); Assert.Throws<ObjectDisposedException>(() => device.NewDatabaseId());
        });
    }
}
