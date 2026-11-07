using Foundation.Data.Doublets.Cli.Engine;
using static Foundation.Data.Doublets.Cli.Tests.PagedTestDevice;

namespace Foundation.Data.Doublets.Cli.Tests;

public class PagedMalformedTests
{
    [Fact]
    public void ChecksummedMetadataRejectsUnsupportedFormatsAndContradictoryCounts()
    {
        foreach (var (offset, value) in new (int, ulong)[] { (0, 0), (8, 2), (12, 1), (80, 1), (32, 3), (40, 2199023255552),
            (48, 253), (48, 0), (56, 7), (56, 0), (64, 0), (64, 253), (32, 1UL << 63) })
        {
            var (image, _) = Fixture("oneGroup"); Set(image.Metadata[1]!, offset, value); Sign(image.Metadata[1]!);
            var device = From(image); Assert.Throws<IOException>(() => new PagedCommitLog(device)); Same(image, device.Live);
        }
        var (foreign, _) = Fixture("oneGroup"); foreign.Metadata[0]![16] ^= 1; Sign(foreign.Metadata[0]!);
        Assert.Throws<IOException>(() => new PagedCommitLog(From(foreign)));
        var (identical, records) = Fixture("oneGroup"); identical.Metadata[0] = identical.Metadata[1]!.ToArray();
        Assert.Equal(records, new PagedCommitLog(From(identical)).Read()); Set(identical.Metadata[0]!, 48, 5); Sign(identical.Metadata[0]!);
        Assert.Throws<IOException>(() => new PagedCommitLog(From(identical)));
        var (empty, _) = Fixture("empty"); Set(empty.Metadata[0]!, 72, 1); Sign(empty.Metadata[0]!);
        Assert.Throws<IOException>(() => new PagedCommitLog(From(empty)));
        var (missing, _) = Fixture("oneGroup"); missing.Metadata = new byte[]?[2]; Assert.Throws<IOException>(() => new PagedCommitLog(From(missing)));
        Assert.Throws<IOException>(() => new PagedCommitLog(From(new Image { Metadata = new byte[]?[] { new byte[1], null } })));
        var (truncated, _) = Fixture("oneGroup"); truncated.Pages.Clear(); Assert.Throws<IOException>(() => new PagedCommitLog(From(truncated)));
    }
    [Fact]
    public void ChecksummedPagesRejectNoncanonicalReferencesPayloadsAndPadding()
    {
        foreach (var (offset, value) in new (int, ulong)[] { (0, 0), (8, 0), (24, 1), (32, 0), (32, 2), (40, 0), (40, 253), (44, 1), (4088, 1),
            (144, 0), (144, 4), (152, 1), (128, 0), (128, 0x8800000000000000), (128, 0x8000000000000000),
            (128, 0x8100000000000100), (128, 0x81000000000000ff), (128, 0x810000000000000a), (136, 3), (144, 0x8000000000000000) })
        {
            var (image, _) = Fixture("oneGroup"); Set(image.Pages[0], offset, value); SignPages(image);
            var device = From(image); Assert.Throws<IOException>(() => new PagedCommitLog(device)); Same(image, device.Live);
        }
        foreach (var (offset, value) in new (int, ulong)[] { (48, 5), (56, 2), (64, 5) })
        {
            var (image, _) = Fixture("oneGroup"); Set(image.Metadata[1]!, offset, value); Sign(image.Metadata[1]!);
            Assert.Throws<IOException>(() => new PagedCommitLog(From(image)));
        }
    }
}
