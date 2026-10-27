using System.Buffers.Binary;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public partial class CheckpointPagesTests
{
    private static IReadOnlyList<string> Inspect(Device[] devices, ulong max = 4) => CheckpointPagedCommitLog.Inspect(devices[0], devices[1], devices[2], max);
    private static Image[] InspectionFixture()
    {
        var (log, devices, _) = Setup(); log.Append("old"); log.Flush(); log.Replace(new[] { "acknowledged\0\ufeff😀" }); return Durable(devices);
    }
    private static void InspectionUnchanged(Device[] devices, Image[] images, Trace trace)
    {
        Assert.All(trace.Calls, call => Assert.True(call.Contains("read", StringComparison.Ordinal) || call.EndsWith("dataLength", StringComparison.Ordinal), call));
        for (int i = 0; i < devices.Length; i++)
        {
            Assert.Equal(images[i].Metadata, devices[i].Live.Metadata); Assert.Equal(images[i].Pages, devices[i].Live.Pages);
            Assert.Equal(images[i].Metadata, devices[i].Durable.Metadata); Assert.Equal(images[i].Pages, devices[i].Durable.Pages);
        }
    }
    [Fact]
    public void InspectionEmptyPartialOwnershipModesAndLimitsNeverInitialize()
    {
        var initial = Devices(); var d = initial.Devices; Assert.Empty(Inspect(d)); InspectionUnchanged(d, new[] { new Image(), new Image(), new Image() }, initial.Trace);
        var partial = InspectionFixture(); partial[0].Metadata = new byte[]?[2];
        foreach (var bank in partial.Skip(1)) { bank.Pages.Clear(); bank.Metadata[1] = null; }
        var current = Devices(partial); Assert.Empty(Inspect(current.Devices)); InspectionUnchanged(current.Devices, partial, current.Trace);
        Assert.Throws<ArgumentNullException>(() => CheckpointPagedCommitLog.Inspect(null!, d[1], d[2]));
        Assert.Throws<ArgumentNullException>(() => CheckpointPagedCommitLog.Inspect(d[0], null!, d[2]));
        Assert.Throws<ArgumentNullException>(() => CheckpointPagedCommitLog.Inspect(d[0], d[1], null!));
        foreach (var duplicate in new[] { new[] { d[0], d[0], d[2] }, new[] { d[0], d[1], d[0] }, new[] { d[0], d[1], d[1] } })
            Assert.Throws<ArgumentException>(() => CheckpointPagedCommitLog.Inspect(duplicate[0], duplicate[1], duplicate[2]));
        initial.Trace.Calls.Clear();
        foreach (ulong max in new[] { 0UL, ulong.MaxValue }) Assert.Throws<ArgumentOutOfRangeException>(() => Inspect(d, max));
        Assert.Empty(initial.Trace.Calls);
        foreach (uint[] flags in new[] { new uint[] { 1, 0, 0 }, new uint[] { 2, 1, 1 }, new uint[] { 0, 2, 2 }, new uint[] { 0, 0, 1 } })
        {
            var probe = Devices(); for (int i = 0; i < 3; i++) probe.Devices[i].MetadataFlags = flags[i];
            Assert.Throws<IOException>(() => Inspect(probe.Devices)); Assert.Empty(probe.Trace.Calls);
        }
    }
    [Fact]
    public void InspectionOwnsPrefixAndPreservesSuffixAndInactiveDamageUntilOrdinaryOpen()
    {
        var images = InspectionFixture(); images[2].Pages.Add(Enumerable.Repeat((byte)7, 4096).ToArray());
        images[1].Metadata = new byte[]?[] { new byte[17], null }; images[1].Pages[0][20] ^= 1;
        var (devices, trace) = Devices(images); var records = Inspect(devices);
        Assert.Equal(new[] { "acknowledged\0\ufeff😀" }, records);
        ((IList<string>)records)[0] = "caller edit"; Assert.Equal(new[] { "acknowledged\0\ufeff😀" }, Inspect(devices));
        InspectionUnchanged(devices, images, trace);
        var ordinary = Open(devices); Assert.Equal(new[] { "acknowledged\0\ufeff😀" }, ordinary.Read()); Assert.Single(devices[2].Live.Pages);
        Assert.Contains(trace.Calls, call => call.EndsWith("truncateData", StringComparison.Ordinal));
    }
    [Fact]
    public void InspectionRejectsCorruptRootsDecisionsPagesAndUnrootedCommitsWithoutWrites()
    {
        var images = InspectionFixture(); var invalid = new List<Image[]>();
        void Add(Action<Image[]> change) { var image = images.Select(item => item.Copy()).ToArray(); change(image); invalid.Add(image); }
        Add(image => image[2].Metadata = new byte[]?[2]); Add(image => image[2].Metadata = images[1].Metadata);
        Add(image => image[2].Pages[0][56] ^= 1); Add(image => image[2].Pages.Clear()); Add(image => image[0].Pages.Add(new byte[4096]));
        Add(image => image[0].Metadata = new byte[]?[2]);
        Add(image => { image[0].Metadata = new byte[]?[2]; image[1].Pages.Clear(); image[2].Pages.Clear(); });
        Add(image => { byte[] raw = image[2].Metadata[1]!.ToArray(); raw[72] ^= 1; PagedTestDevice.Sign(raw); image[2].Metadata[0] = raw; });
        using var vectors = Vectors();
        foreach (var root in vectors.RootElement.GetProperty("invalid").EnumerateArray()) Add(image => image[0].Metadata[0] = Convert.FromHexString(root.GetString()!));
        Add(image => image[0].Metadata[1] = Convert.FromHexString(vectors.RootElement.GetProperty("foreignDecision").GetString()!));
        foreach (var image in invalid)
        {
            var (devices, trace) = Devices(image); Assert.Throws<IOException>(() => Inspect(devices)); InspectionUnchanged(devices, image, trace);
        }
    }
    [Fact]
    public void InspectionLowerBoundFailsWithoutTruncationAndKeepsDevicesUsable()
    {
        var initial = Setup(); initial.Log.Append(new string('x', 2000)); initial.Log.Flush(); var images = Durable(initial.Devices);
        var (devices, trace) = Devices(images); Assert.Contains("exceed selected", Assert.Throws<IOException>(() => Inspect(devices, 1)).Message);
        InspectionUnchanged(devices, images, trace); Assert.Equal(new[] { new string('x', 2000) }, Inspect(devices));
    }
    [Fact]
    public void InspectionEveryReadFailurePreservesEvidenceAndPermitsBorrowedRetry()
    {
        var images = InspectionFixture(); var probe = Devices(images); Inspect(probe.Devices); int operations = probe.Trace.Calls.Count; Assert.InRange(operations, 6, 20);
        for (int at = 1; at <= operations; at++) foreach (bool after in new[] { false, true })
        {
            var (devices, trace) = Devices(images); trace.Fault = (at, after); Assert.Throws<IOException>(() => Inspect(devices)); InspectionUnchanged(devices, images, trace);
            trace.Fault = null; Assert.Equal(new[] { "acknowledged\0\ufeff😀" }, Inspect(devices)); InspectionUnchanged(devices, images, trace);
        }
    }
    [Fact]
    public void InspectionActualMirroredRootAndBanksKeepRepairsStagedUntilValidatedOpen()
    {
        static (IPageDevice Root, IPageDevice First, IPageDevice Second) Wrap(Device[] d) =>
            (new MirroredCheckpointRoot(d[0], d[5], 1), new MirroredPageDevice(d[1], d[3]), new MirroredPageDevice(d[2], d[4]));
        var initial = Devices(Enumerable.Range(0, 6).Select(_ => new Image()).ToArray()); var owners = Wrap(initial.Devices);
        var log = new CheckpointPagedCommitLog(owners.Root, owners.First, owners.Second, 4); log.Append("old"); log.Flush(); log.Replace(new[] { "acknowledged" });
        var images = Durable(initial.Devices); images[0].Metadata[1]![120] ^= 1; images[2].Pages[0][100] ^= 1;
        var (devices, trace) = Devices(images); owners = Wrap(devices);
        Assert.Equal(new[] { "acknowledged" }, CheckpointPagedCommitLog.Inspect(owners.Root, owners.First, owners.Second, 4)); InspectionUnchanged(devices, images, trace);
        owners = Wrap(devices); log = new CheckpointPagedCommitLog(owners.Root, owners.First, owners.Second, 4);
        Assert.Equal(new[] { "acknowledged" }, log.Read()); Assert.Equal(devices[0].Live.Metadata, devices[5].Live.Metadata); Assert.Equal(devices[2].Live.Pages, devices[4].Live.Pages);
    }
}
