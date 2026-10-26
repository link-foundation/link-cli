using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class NativeCheckpointFactoryTests
{
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void SelectedCheckpointFactoryRejectsImplicitOrdinaryJournalConversion(bool mirrored)
    {
        string directory = Directory.CreateTempSubdirectory("checkpoint-format-").FullName, path = Path.Combine(directory, "journal");
        try
        {
            using (var device = new FilePageDevice(path)) { var ordinary = new PagedCommitLog(device); ordinary.Append("ordinary 雪"); ordinary.Flush(); }
            var files = new[] { path, path + ".meta0", path + ".meta1" }; var before = files.Select(File.ReadAllBytes).ToArray();
            Assert.Throws<IOException>(() => new FileCheckpointPagedCommitLog(path, mirrored));
            for (int index = 0; index < files.Length; index++) Assert.Equal(before[index], File.ReadAllBytes(files[index]));
            Assert.False(File.Exists(path + ".writer"));
            using (var device = new FilePageDevice(path)) { var reopened = new PagedCommitLog(device); Assert.Equal(new[] { "ordinary 雪" }, reopened.Read()); reopened.Append("following"); reopened.Flush(); }
            string selected = Path.Combine(directory, "checkpoint");
            using (var checkpoint = new FileCheckpointPagedCommitLog(selected, mirrored)) checkpoint.Replace(new[] { "selected" });
            using (var device = new FilePageDevice(selected)) Assert.Throws<IOException>(() => new PagedCommitLog(device));
            using var again = new FileCheckpointPagedCommitLog(selected, mirrored); Assert.Equal(new[] { "selected" }, again.Read());
        }
        finally { Directory.Delete(directory, true); }
    }
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void FactoryOwnsEveryFixedDeviceAndNeverFlushesOnClose(bool mirrored)
    {
        string directory = Path.Combine(Path.GetTempPath(), Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            string path = Path.Combine(directory, "journal");
            using var log = new FileCheckpointPagedCommitLog(path, mirrored, 64);
            Assert.True(log.SupportsDurableFlush); Assert.True(log.SupportsReplacement);
            foreach (string suffix in mirrored ? new[] { "", ".bank0", ".bank0.mirror", ".bank1", ".bank1.mirror" } : new[] { "", ".bank0", ".bank1" })
                Assert.Throws<IOException>(() => new FilePageDevice(path + suffix));
            Assert.True(log.Replace(new[] { "acknowledged 雪" })); Assert.False(log.Replace(new[] { "acknowledged 雪" }));
            log.Append("private pending"); log.Dispose(); log.Dispose();
            Assert.Throws<ObjectDisposedException>(() => { _ = log.Epoch; });
            Assert.Throws<ObjectDisposedException>(() => { log.Read(); }); Assert.Throws<ObjectDisposedException>(() => { log.Append("closed"); });
            Assert.Throws<ObjectDisposedException>(() => { log.Flush(); }); Assert.Throws<ObjectDisposedException>(() => { log.Replace(Array.Empty<string>()); });
            using var reopened = new FileCheckpointPagedCommitLog(path, mirrored);
            Assert.Equal(new[] { "acknowledged 雪" }, reopened.Read());
            reopened.Append("following"); reopened.Flush(); Assert.True(reopened.Replace(new[] { "compacted" })); reopened.Dispose();
            using var again = new FileCheckpointPagedCommitLog(path, mirrored); Assert.Equal(new[] { "compacted" }, again.Read());
        }
        finally { Directory.Delete(directory, true); }
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void EveryFenceIsAcquiredBeforeSelectedBankRepairOrReset(bool mirrored)
    {
        string directory = Directory.CreateTempSubdirectory("checkpoint-fences-").FullName, path = Path.Combine(directory, "journal");
        try
        {
            using var initial = new FileCheckpointPagedCommitLog(path, mirrored);
            initial.Replace(new[] { "previous" }); initial.Replace(new[] { "acknowledged 雪" });
            string selected = path + $".bank{initial.Epoch % 2}"; initial.Dispose();
            if (mirrored) { File.WriteAllBytes(selected, File.ReadAllBytes(selected)[..19]); File.WriteAllText(selected + ".meta0", "torn"); }
            var devices = (mirrored ? new[] { "", ".bank0", ".bank0.mirror", ".bank1", ".bank1.mirror" } : new[] { "", ".bank0", ".bank1" }).Select(suffix => path + suffix).ToArray();
            var files = devices.SelectMany(name => new[] { name, name + ".meta0", name + ".meta1" }).ToArray();
            var before = files.Select(File.ReadAllBytes).ToArray();
            foreach (string foreign in devices)
            {
                File.WriteAllText(foreign + ".writer", "foreign owner");
                Assert.Throws<IOException>(() => new FileCheckpointPagedCommitLog(path, mirrored));
                for (int index = 0; index < files.Length; index++) Assert.Equal(before[index], File.ReadAllBytes(files[index]));
                foreach (string name in devices) Assert.Equal(name == foreign, File.Exists(name + ".writer"));
                Assert.Equal("foreign owner", File.ReadAllText(foreign + ".writer")); File.Delete(foreign + ".writer");
            }
            using var recovered = new FileCheckpointPagedCommitLog(path, mirrored); Assert.Equal(new[] { "acknowledged 雪" }, recovered.Read());
            if (mirrored) Assert.Equal(File.ReadAllBytes(selected), File.ReadAllBytes(selected + ".mirror"));
            recovered.Append("following"); recovered.Flush(); Assert.True(recovered.Replace(new[] { "compacted" })); recovered.Dispose();
            using var again = new FileCheckpointPagedCommitLog(path, mirrored); Assert.Equal(new[] { "compacted" }, again.Read());
        }
        finally { Directory.Delete(directory, true); }
    }
}
