using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public partial class CheckpointPagesTests
{
    [Fact]
    public void MirroredRootRecoversAcknowledgedEpochAfterOneCopyDamage()
    {
        var (devices, _) = Devices(new[] { new Image(), new Image(), new Image(), new Image() });
        var root = new MirroredCheckpointRoot(devices[0], devices[3], 0);
        var log = new CheckpointPagedCommitLog(root, devices[1], devices[2], maxPages: 4);
        log.Append("old"); log.Flush(); log.Replace(new[] { "acknowledged" });
        var images = Durable(devices); images[0].Metadata[1]![120] ^= 1;
        var (reopened, _) = Devices(images);
        root = new MirroredCheckpointRoot(reopened[0], reopened[3], 0);
        log = new CheckpointPagedCommitLog(root, reopened[1], reopened[2], maxPages: 4);
        Assert.Equal(1UL, log.Epoch); Assert.Equal(new[] { "acknowledged" }, log.Read());
        Assert.Equal(Durable(reopened)[0].Metadata, Durable(reopened)[3].Metadata);
    }
}
