using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class NativeCheckpointRootTests
{
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void NativeRootMirroringRecoversTheAcknowledgedEpochWithEitherBankMode(bool mirroredBanks)
    {
        string directory = Directory.CreateTempSubdirectory("checkpoint-root-").FullName, path = Path.Combine(directory, "journal");
        try
        {
            using var initial = new FileCheckpointPagedCommitLog(path, mirroredBanks, mirroredRoot: true);
            initial.Replace(new[] { "old" }); initial.Replace(new[] { "acknowledged 雪" }); initial.Dispose();
            Assert.True(File.Exists(path + ".mirror.meta0")); File.WriteAllText(path + ".meta0", "torn");
            using var recovered = new FileCheckpointPagedCommitLog(path, mirroredBanks, mirroredRoot: true);
            Assert.Equal(2UL, recovered.Epoch); Assert.Equal(new[] { "acknowledged 雪" }, recovered.Read());
            recovered.Append("following"); recovered.Flush(); recovered.Replace(new[] { "next", "following" }); recovered.Dispose();
            Assert.Equal(File.ReadAllBytes(path + ".meta1"), File.ReadAllBytes(path + ".mirror.meta1"));
            using var again = new FileCheckpointPagedCommitLog(path, mirroredBanks, mirroredRoot: true);
            Assert.Equal(new[] { "next", "following" }, again.Read());
        }
        finally { Directory.Delete(directory, true); }
    }
}
