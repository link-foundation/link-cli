using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class NativeCheckpointFactoryTests
{
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
            Assert.Throws<ObjectDisposedException>(() => { _ = log.Read(); }); Assert.Throws<ObjectDisposedException>(() => { log.Append("closed"); });
            Assert.Throws<ObjectDisposedException>(() => { log.Flush(); }); Assert.Throws<ObjectDisposedException>(() => { _ = log.Replace(Array.Empty<string>()); });
            using var reopened = new FileCheckpointPagedCommitLog(path, mirrored);
            Assert.Equal(new[] { "acknowledged 雪" }, reopened.Read());
            reopened.Append("following"); reopened.Flush(); Assert.True(reopened.Replace(new[] { "compacted" })); reopened.Dispose();
            using var again = new FileCheckpointPagedCommitLog(path, mirrored); Assert.Equal(new[] { "compacted" }, again.Read());
        }
        finally { Directory.Delete(directory, true); }
    }
}
