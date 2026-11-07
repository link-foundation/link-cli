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

    private static string[] Devices(string path, bool mirroredBanks, bool mirroredRoot) =>
        new[] { path }.Concat(mirroredRoot ? new[] { path + ".mirror" } : Array.Empty<string>())
        .Concat(Enumerable.Range(0, 2).SelectMany(bank => mirroredBanks ? new[] { path + $".bank{bank}", path + $".bank{bank}.mirror" } : new[] { path + $".bank{bank}" })).ToArray();
    private static string[] Files(IEnumerable<string> devices) => devices.SelectMany(name => new[] { name, name + ".meta0", name + ".meta1" }).ToArray();
    private static byte[][] Snapshot(IEnumerable<string> files) => files.Select(File.ReadAllBytes).ToArray();
    private static void Unchanged(string[] files, byte[][] before) { for (int i = 0; i < files.Length; i++) Assert.Equal(before[i], File.ReadAllBytes(files[i])); }

    [Theory]
    [InlineData(false, false)]
    [InlineData(true, false)]
    [InlineData(false, true)]
    [InlineData(true, true)]
    public void EverySelectedFencePrecedesRootAndBankRepair(bool mirroredBanks, bool mirroredRoot)
    {
        string directory = Directory.CreateTempSubdirectory("checkpoint-root-fences-").FullName, path = Path.Combine(directory, "journal");
        try
        {
            using var initial = new FileCheckpointPagedCommitLog(path, mirroredBanks, mirroredRoot);
            initial.Replace(new[] { "previous" }); initial.Replace(new[] { "acknowledged 雪" }); initial.Dispose();
            if (mirroredRoot) File.WriteAllText(path + ".meta0", "torn root");
            if (mirroredBanks) { File.WriteAllBytes(path + ".bank0", File.ReadAllBytes(path + ".bank0")[..19]); File.WriteAllText(path + ".bank0.meta0", "torn bank"); }
            var devices = Devices(path, mirroredBanks, mirroredRoot); var files = Files(devices); var before = Snapshot(files);
            foreach (string foreign in devices)
            {
                File.WriteAllText(foreign + ".writer", "foreign owner");
                Assert.ThrowsAny<IOException>(() => new FileCheckpointPagedCommitLog(path, mirroredBanks, mirroredRoot)); Unchanged(files, before);
                foreach (string name in devices) Assert.Equal(name == foreign, File.Exists(name + ".writer"));
                Assert.Equal("foreign owner", File.ReadAllText(foreign + ".writer")); File.Delete(foreign + ".writer");
            }
            using var recovered = new FileCheckpointPagedCommitLog(path, mirroredBanks, mirroredRoot);
            Assert.Equal(new[] { "acknowledged 雪" }, recovered.Read()); recovered.Dispose();
            if (mirroredRoot) Assert.Equal(File.ReadAllBytes(path + ".meta0"), File.ReadAllBytes(path + ".mirror.meta0"));
            if (mirroredBanks) Assert.Equal(File.ReadAllBytes(path + ".bank0"), File.ReadAllBytes(path + ".bank0.mirror"));
            using var following = new FileCheckpointPagedCommitLog(path, mirroredBanks, mirroredRoot);
            following.Append("following"); following.Flush(); following.Replace(new[] { "compacted" }); following.Dispose();
            using var again = new FileCheckpointPagedCommitLog(path, mirroredBanks, mirroredRoot); Assert.Equal(new[] { "compacted" }, again.Read());
        }
        finally { Directory.Delete(directory, true); }
    }

    [Theory]
    [InlineData(false, false)]
    [InlineData(true, false)]
    [InlineData(false, true)]
    [InlineData(true, true)]
    public void RootAndBankModeConversionsPreserveExistingBytes(bool mirroredBanks, bool mirroredRoot)
    {
        string directory = Directory.CreateTempSubdirectory("checkpoint-root-mode-").FullName, path = Path.Combine(directory, "journal");
        try
        {
            using var initial = new FileCheckpointPagedCommitLog(path, mirroredBanks, mirroredRoot); initial.Replace(new[] { "acknowledged" }); initial.Dispose();
            var files = Files(Devices(path, mirroredBanks, mirroredRoot)); var before = Snapshot(files);
            foreach (var wrong in new[] { (Banks: mirroredBanks, Root: !mirroredRoot), (Banks: !mirroredBanks, Root: mirroredRoot) })
            {
                var error = Assert.ThrowsAny<IOException>(() => new FileCheckpointPagedCommitLog(path, wrong.Banks, wrong.Root));
                Assert.Contains("flags", error.Message, StringComparison.OrdinalIgnoreCase); Unchanged(files, before);
                foreach (string name in Devices(path, true, true)) Assert.False(File.Exists(name + ".writer"));
            }
            using var again = new FileCheckpointPagedCommitLog(path, mirroredBanks, mirroredRoot); Assert.Equal(new[] { "acknowledged" }, again.Read());
        }
        finally { Directory.Delete(directory, true); }
    }
}
