using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class MirroredFileTests
{
    [Fact]
    public void NativeRecoveryRepairsCommittedDataAndMetadataThenAcceptsFollowingWrites()
    {
        string directory = Directory.CreateTempSubdirectory("mirror-file-").FullName, path = Path.Combine(directory, "journal");
        try
        {
            string record = string.Concat(Enumerable.Repeat("雪", 2100));
            using (var device = new FileMirroredPageDevice(path))
            { var log = new PagedCommitLog(device); log.Append(record); log.Flush(); log.Append("\ufeff\0😀"); log.Flush(); Assert.ThrowsAny<IOException>(() => new FileMirroredPageDevice(path)); }
            byte[] healthy = File.ReadAllBytes(path); File.WriteAllBytes(path, healthy[..19]); File.WriteAllBytes(path + ".meta0", new byte[17]);
            using (var device = new FileMirroredPageDevice(path))
            { var log = new PagedCommitLog(device); Assert.Equal(new[] { record, "\ufeff\0😀" }, log.Read()); log.Append("next"); log.Flush(); }
            Assert.Equal(File.ReadAllBytes(path), File.ReadAllBytes(path + ".mirror"));
            using (var device = new FilePageDevice(path)) Assert.Throws<IOException>(() => new PagedCommitLog(device));
            using (var device = new FilePageDevice(path))
            {
                byte[] before = File.ReadAllBytes(path); Assert.Throws<IOException>(() => device.RepairPage(ulong.MaxValue, new byte[4096]));
                Assert.Throws<IOException>(() => device.RepairPage(99, new byte[4096])); Assert.Throws<IOException>(() => device.RepairPage(0, new byte[1]));
                Assert.Equal(before, File.ReadAllBytes(path)); device.Dispose(); Assert.Throws<ObjectDisposedException>(() => device.RepairPage(0, new byte[4096]));
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
    [Fact]
    public void PartialAcquisitionPreservesForeignOwnersAndUnrelatedData()
    {
        string directory = Directory.CreateTempSubdirectory("mirror-owner-").FullName, path = Path.Combine(directory, "journal");
        try
        {
            File.WriteAllText(path + ".mirror.writer", "foreign"); Assert.ThrowsAny<IOException>(() => new FileMirroredPageDevice(path));
            Assert.False(File.Exists(path + ".writer")); Assert.Equal("foreign", File.ReadAllText(path + ".mirror.writer"));
            File.Delete(path + ".mirror.writer"); File.WriteAllText(path, "unrelated");
            using (var device = new FileMirroredPageDevice(path)) Assert.Throws<IOException>(() => new PagedCommitLog(device));
            Assert.Equal("unrelated", File.ReadAllText(path)); Assert.False(File.Exists(path + ".writer")); Assert.False(File.Exists(path + ".mirror.writer"));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
    private sealed class CloseFaultFile(string path, bool owner) : FileStream(path, owner ? FileMode.CreateNew : FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.Read, bufferSize: 1)
    {
        internal int Closed;
        protected override void Dispose(bool disposing) { base.Dispose(disposing); if (disposing) { Closed++; throw new IOException("injected close"); } }
    }
    [Fact]
    public void InitializationAndCleanupErrorsAreBothPreserved()
    {
        string directory = Directory.CreateTempSubdirectory("mirror-init-").FullName, path = Path.Combine(directory, "journal"); var files = new List<CloseFaultFile>();
        FilePageDevice Open(string name)
        {
            if (name.EndsWith(".mirror", StringComparison.Ordinal)) throw new IOException("injected acquisition");
            return new(name, (file, owner) => { var result = new CloseFaultFile(file, owner); files.Add(result); return result; }, _ => { });
        }
        try
        {
            var error = Assert.Throws<AggregateException>(() => new FileMirroredPageDevice(path, Open));
            Assert.Equal(2, error.InnerExceptions.Count); Assert.Contains("injected acquisition", error.InnerExceptions[0].Message);
            Assert.Equal(4, files.Count); Assert.All(files, f => Assert.Equal(1, f.Closed)); Assert.True(File.Exists(path + ".writer"));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
    [Fact]
    public void BothCleanupFailuresAttemptEveryHandleAndRetainUncertainOwners()
    {
        string directory = Directory.CreateTempSubdirectory("mirror-close-").FullName, path = Path.Combine(directory, "journal");
        var files = new List<CloseFaultFile>();
        FilePageDevice Open(string name) => new(name, (file, owner) => { var result = new CloseFaultFile(file, owner); files.Add(result); return result; }, _ => { });
        try
        {
            var device = new FileMirroredPageDevice(path, Open); var error = Assert.Throws<AggregateException>(() => device.Dispose());
            Assert.Equal(2, error.InnerExceptions.Count); Assert.Equal(8, files.Count); Assert.All(files, f => Assert.Equal(1, f.Closed));
            Assert.True(File.Exists(path + ".writer")); Assert.True(File.Exists(path + ".mirror.writer")); device.Dispose(); Assert.All(files, f => Assert.Equal(1, f.Closed));
            Assert.Throws<IOException>(() => device.SyncData());
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
