using Foundation.Data.Doublets.Cli.Engine;
using static Foundation.Data.Doublets.Cli.Tests.PagedTestDevice;
using static Foundation.Data.Doublets.Cli.Tests.MirroredTestDevice;
namespace Foundation.Data.Doublets.Cli.Tests;

public class MirroredPagedTests
{
    [Fact]
    public void IndependentModeVectorsAndEverySingleCopyDamageRepair()
    {
        var generated = new Pair(new(), new()); var writer = generated.Open();
        foreach (var device in generated.Devices) Same(MirroredFixture("empty").Image, device.Durable);
        foreach (string record in MirroredFixture("oneGroup").Records) writer.Append(record); writer.Flush();
        foreach (var device in generated.Devices) Same(MirroredFixture("oneGroup").Image, device.Durable);
        writer.Append(""); writer.Flush(); foreach (var device in generated.Devices) Same(MirroredFixture("twoGroups").Image, device.Durable);
        foreach (string name in new[] { "empty", "oneGroup", "twoGroups" })
        {
            var (image, expected) = MirroredFixture(name); var pair = new Pair(image, image);
            Assert.Equal(expected, pair.Open().Read()); Assert.Empty(pair.Mutations.Calls);
            Assert.Throws<IOException>(() => new PagedCommitLog(pair.Devices[0])); pair.Following(expected);
        }
        using var json = Wire(); var (healthy, records) = MirroredFixture("twoGroups");
        foreach (var fault in json.RootElement.GetProperty("singleCopyFaults").EnumerateArray())
        {
            var images = new[] { healthy.Copy(), healthy.Copy() }; var image = images[fault.GetProperty("side").GetInt32()];
            int N(string key) => fault.GetProperty(key).GetInt32();
            switch (fault.GetProperty("kind").GetString())
            {
                case "page": image.Pages[N("page")][N("offset")] ^= 1; break;
                case "metadata": image.Metadata[N("slot")] = image.Metadata[N("slot")]![..N("length")]; break;
                case "truncated": image.Pages.RemoveRange(N("pages"), image.Pages.Count - N("pages")); break;
                case "stale": image.Pages[N("page")][64] ^= 1; Set(image.Pages[N("page")], 56, image.Pages[N("page")].Take(56).Concat(image.Pages[N("page")].Skip(64)).Aggregate(14695981039346656037UL, (sum, b) => unchecked((sum ^ b) * 1099511628211))); break;
                default: throw new InvalidOperationException("unknown vector");
            }
            var pair = new Pair(images[0], images[1]); Assert.Equal(records, pair.Open().Read());
            foreach (var d in pair.Devices) Same(healthy, d.Durable);
            pair.Mutations.Calls.Clear(); pair.Open(); Assert.Empty(pair.Mutations.Calls); pair.Following(records);
        }
    }
    [Fact]
    public void EveryFlushFaultBeforeAfterKeepsAnOldOrCompleteGroup()
    {
        var (image, prior) = MirroredFixture("oneGroup"); string next = string.Concat(Enumerable.Repeat("雪", 700));
        var probe = new Pair(image, image); var complete = probe.Open(); probe.Mutations.Calls.Clear(); complete.Append(next); complete.Flush();
        for (int at = 1; at <= probe.Mutations.Calls.Count; at++) foreach (bool after in new[] { false, true })
            {
                var pair = new Pair(image, image); var log = pair.Open(); pair.Mutations.Calls.Clear(); pair.Mutations.Fault = (at, after);
                log.Append(next); Assert.Throws<IOException>(() => log.Flush()); Assert.Throws<IOException>(() => log.Read()); Assert.Throws<IOException>(() => log.Append("retry")); Assert.Throws<IOException>(() => log.Flush());
                var recovered = pair.Crash(); var actual = recovered.Open().Read(); Assert.Equal(actual.Count > prior.Length ? prior.Append(next) : prior, actual); recovered.Following(actual);
            }
    }
    [Fact]
    public void EveryRepairFaultBeforeAfterPreservesASurvivingSource()
    {
        var (healthy, records) = MirroredFixture("twoGroups");
        foreach (int side in new[] { 0, 1 })
        {
            var images = new[] { healthy.Copy(), healthy.Copy() }; images[side].Pages.Clear(); images[side].Metadata[0] = new byte[17];
            var probe = new Pair(images[0], images[1]); probe.Open();
            for (int at = 1; at <= probe.Mutations.Calls.Count; at++) foreach (bool after in new[] { false, true })
                {
                    var pair = new Pair(images[0], images[1]); pair.Mutations.Fault = (at, after); Assert.Throws<IOException>(() => pair.Open());
                    Same(healthy, pair.Devices[1 - side].Durable); var recovered = pair.Crash(); Assert.Equal(records, recovered.Open().Read()); recovered.Following(records);
                }
        }
    }
    [Fact]
    public void BothCopyLossCanonicalDamageAndConflictsRejectBeforeRepair()
    {
        var (healthy, _) = MirroredFixture("twoGroups"); var damaged = healthy.Copy(); damaged.Pages[0][56] ^= 1;
        var pair = new Pair(damaged, damaged); Assert.Throws<IOException>(() => pair.Open()); Assert.Empty(pair.Mutations.Calls);
        var invalid = MirroredFixture("oneGroup").Image; invalid.Pages[0][80] = 0xff; SignPages(invalid);
        var bad = invalid.Copy(); bad.Pages[0][56] ^= 1; bad.Pages.Add(new byte[4096]); pair = new Pair(invalid, bad);
        Assert.Throws<IOException>(() => pair.Open()); Assert.Empty(pair.Mutations.Calls);
        foreach (int offset in new[] { 16, 72 }) { bad = healthy.Copy(); bad.Metadata[0]![offset] ^= 1; Sign(bad.Metadata[0]!); pair = new Pair(healthy, bad); Assert.Throws<IOException>(() => pair.Open()); Assert.Empty(pair.Mutations.Calls); }
    }
    [Fact]
    public void OneUnreadableCopyRepairsWhileBothUnreadableCopiesReject()
    {
        var (image, records) = MirroredFixture("twoGroups");
        foreach (int side in new[] { 0, 1 })
        {
            var pair = new Pair(image, image); pair.Devices[side].ReadError = true;
            Assert.Equal(records, pair.Open().Read()); pair.Devices[side].ReadError = false; pair.Following(records);
        }
        var both = new Pair(image, image); foreach (var device in both.Devices) device.ReadError = true;
        Assert.Throws<IOException>(() => both.Open()); Assert.Empty(both.Mutations.Calls);
    }
    [Fact]
    public void ChangedSourcesInvalidLengthsUnsupportedFlagsAndStaleDecisionsAreChecked()
    {
        var (healthy, records) = MirroredFixture("twoGroups"); var pair = new Pair(healthy, healthy);
        foreach (long length in new[] { -1L, long.MaxValue }) { pair.Devices[0].Length = length; Assert.Throws<IOException>(() => pair.Open()); Assert.Empty(pair.Mutations.Calls); }
        pair.Devices[0].Length = null; pair.Devices[0].MetadataError = true; Assert.Equal(records, pair.Open().Read()); pair.Devices[0].MetadataError = false;
        var older = healthy.Copy(); older.Metadata[0] = MirroredFixture("empty").Image.Metadata[0]; pair = new Pair(older, healthy); Assert.Equal(records, pair.Open().Read()); Same(healthy, pair.Devices[0].Durable);
        var unsupported = healthy.Copy(); Set(unsupported.Metadata[0]!, 8, 1UL | (2UL << 32)); Sign(unsupported.Metadata[0]!);
        pair = new Pair(healthy, unsupported); Assert.Throws<IOException>(() => pair.Open()); Assert.Empty(pair.Mutations.Calls);
        var bad = healthy.Copy(); bad.Pages.Clear(); pair = new Pair(healthy, bad); int reads = 0;
        pair.Devices[0].BeforeRead = index => { if (index == 0 && ++reads == 2) pair.Devices[0].Live.Pages[0][64] ^= 1; };
        Assert.Throws<IOException>(() => pair.Open()); Assert.Empty(pair.Mutations.Calls);
    }
    [Fact]
    public void EveryTailRepairFailureFencesTheDeviceAndCanRecoverAgain()
    {
        var (healthy, records) = MirroredFixture("twoGroups"); var extra = healthy.Copy(); extra.Pages.Add(new byte[4096]);
        for (int at = 1; at <= 4; at++) foreach (bool after in new[] { false, true })
            {
                var pair = new Pair(extra, extra); pair.Mutations.Fault = (at, after); var device = new MirroredPageDevice(pair.Devices[0], pair.Devices[1]);
                Assert.Throws<IOException>(() => new PagedCommitLog(device)); Assert.Throws<IOException>(() => device.SyncData()); Assert.Throws<IOException>(() => device.ReadMetadata(0));
                Assert.Throws<IOException>(() => device.ReadCheckedPage(0, 0)); Assert.Throws<IOException>(() => device.CompleteRecovery()); Assert.Throws<IOException>(() => device.NewDatabaseId());
                Assert.Throws<IOException>(() => device.DataLength); pair.Crash().Following(records);
            }
        var okay = new Pair(healthy, healthy); var raw = new MirroredPageDevice(okay.Devices[0], okay.Devices[1]); Assert.Throws<IOException>(() => raw.ReadMetadata(2));
    }
    [Fact]
    public void SelectedFlushCostsStayConstantAndRepairCapabilitiesAreExplicit()
    {
        foreach (int history in new[] { 0, 1, 32 })
        {
            var pair = new Pair(new(), new()); var log = pair.Open(); for (int i = 0; i < history; i++) { log.Append("prior"); log.Flush(); }
            var before = pair.Devices[0].Live.Copy(); pair.Mutations.Calls.Clear(); pair.Mutations.Reads.Clear(); log.Append("next"); log.Flush();
            foreach (string name in new[] { "writePage", "syncData", "writeMetadata", "syncMetadata" }) Assert.Equal(2, pair.Mutations.Calls.Count(c => c.Contains(name, StringComparison.Ordinal)));
            Assert.Empty(pair.Mutations.Reads);
            for (int i = 0; i < history; i++) Assert.Equal(before.Pages[i], pair.Devices[0].Live.Pages[i]);
            var device = new MirroredPageDevice(pair.Devices[0], pair.Devices[1]); Assert.Equal(1U, device.MetadataFlags); Assert.True(device.SupportsDurableFlush); Assert.Throws<IOException>(() => device.ReadPage(0));
            pair.Devices[0].SupportsDurableFlush = false; Assert.False(device.SupportsDurableFlush);
            Assert.Throws<ArgumentException>(() => new MirroredPageDevice(pair.Devices[0], pair.Devices[0])); Assert.Throws<ArgumentNullException>(() => new MirroredPageDevice(null!, pair.Devices[1]));
        }
    }
}
