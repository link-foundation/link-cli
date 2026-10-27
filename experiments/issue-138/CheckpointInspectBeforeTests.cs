using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;
public class CheckpointInspectBeforeTests
{
    [Fact]
    public void EmptyInspectionDoesNotInitializeDevices()
    {
        var root = new PagedTestDevice(); var first = new PagedTestDevice(); var second = new PagedTestDevice();
        Assert.Empty(CheckpointPagedCommitLog.Inspect(root, first, second, 128));
        Assert.Null(root.ReadMetadata(0)); Assert.Null(first.ReadMetadata(0)); Assert.Null(second.ReadMetadata(0));
    }
}
