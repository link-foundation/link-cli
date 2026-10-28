using Foundation.Data.Doublets.Cli.Engine;
using static Foundation.Data.Doublets.Cli.Tests.PagedTestDevice;

namespace Foundation.Data.Doublets.Cli.Tests;

public class PagedPersistenceTests
{
    [Fact]
    public void GeneratedBytesMatchIndependentVectorsAndStrictPendingText()
    {
        var device = new PagedTestDevice(); var log = new PagedCommitLog(device); Same(Fixture("empty").Image, device.Live);
        foreach (string value in Fixture("oneGroup").Records) log.Append(value);
        Assert.Equal(Fixture("oneGroup").Records, log.Read()); log.Flush(); Same(Fixture("oneGroup").Image, device.Live);
        log.Append(""); log.Flush(); Same(Fixture("twoGroups").Image, device.Live);
        foreach (string invalid in new[] { "\r", "a\nb", "\ud800", "\udfff" }) Assert.ThrowsAny<ArgumentException>(() => log.Append(invalid));
        Assert.Equal(Fixture("twoGroups").Records, log.Read());
    }
    [Fact]
    public void EveryFiniteFlushFaultFencesAndPreservesTheAcknowledgedPrefix()
    {
        var (image, prior) = Fixture("oneGroup"); var group = new[] { string.Concat(Enumerable.Repeat("雪", 700)), "", "following\0\ufeff" };
        var probe = From(image); var complete = new PagedCommitLog(probe);
        foreach (string record in group) complete.Append(record); probe.Calls.Clear(); complete.Flush(); int operations = probe.Calls.Count;
        Assert.InRange(operations, 5, 8);
        for (int at = 1; at <= operations; at++)
            foreach (bool after in new[] { false, true })
            {
                var device = From(image); var log = new PagedCommitLog(device);
                foreach (string record in group) log.Append(record);
                device.Calls.Clear(); device.Fault = (at, after); Assert.Contains("injected", Assert.Throws<IOException>(() => log.Flush()).Message);
                Assert.Throws<IOException>(() => log.Read()); Assert.Throws<IOException>(() => log.Append("retry")); Assert.Throws<IOException>(() => log.Flush());
                var recovered = device.Crash(); var reopened = new PagedCommitLog(recovered); var survived = reopened.Read();
                bool published = survived.Count > prior.Length; Assert.Equal(published ? prior.Concat(group) : prior, survived);
                if (at == operations && after) Assert.True(published);
                reopened.Append("next"); reopened.Flush(); Assert.Equal(survived.Concat(new[] { "next" }), new PagedCommitLog(recovered.Crash()).Read());
            }
    }
    [Fact]
    public void DamagedCommittedPagesNeverFallBackToAnOlderRoot()
    {
        foreach (int offset in new[] { 0, 8, 24, 32, 40, 44, 48, 56, 64, 4095 })
        {
            var (image, _) = Fixture("twoGroups"); image.Pages[1][offset] ^= 1; var device = From(image);
            Assert.Throws<IOException>(() => new PagedCommitLog(device)); Same(image, device.Live);
        }
    }
    [Fact]
    public void TornUnacknowledgedMetadataDiscardsOnlyItsUnpublishedTail()
    {
        foreach (int length in new[] { 0, 1, 8, 16, 32, 64, 119, 120, 127 })
        {
            var (image, _) = Fixture("twoGroups"); image.Metadata[0] = image.Metadata[0]![..length];
            var device = From(image); var log = new PagedCommitLog(device); Assert.Equal(Fixture("oneGroup").Records, log.Read());
            Assert.Single(device.Live.Pages); log.Append("successor"); log.Flush();
            Assert.Equal(Fixture("oneGroup").Records.Concat(new[] { "successor" }), new PagedCommitLog(device.Crash()).Read());
        }
    }
    [Fact]
    public void StrictQuotaEmptyFlushAndReadFailuresPreserveOwnership()
    {
        var device = new PagedTestDevice();
        foreach (ulong limit in new[] { 0UL, PagedCommitLog.MaximumPages + 1, ulong.MaxValue }) Assert.Throws<ArgumentOutOfRangeException>(() => new PagedCommitLog(device, limit));
        Assert.Throws<ArgumentNullException>(() => new PagedCommitLog(null!));
        var log = new PagedCommitLog(device, 1); int before = device.Calls.Count; log.Flush(); Assert.Equal(before, device.Calls.Count);
        log.Append("accepted\0😀"); log.Flush(); Assert.Throws<IOException>(() => log.Append("over quota")); Assert.Equal(new[] { "accepted\0😀" }, log.Read());
        Assert.Throws<IOException>(() => new PagedCommitLog(From(Fixture("twoGroups").Image), 1));
        device.ReadError = true; Assert.Throws<IOException>(() => log.Read()); device.ReadError = false;
        Assert.Throws<IOException>(() => log.Append("rejected")); Assert.Throws<IOException>(() => log.Flush());
        Assert.False(new PagedCommitLog(new PagedTestDevice { SupportsDurableFlush = false }).SupportsDurableFlush);
    }
    [Fact]
    public void FlushWritesOnlyTheNewGroupRegardlessOfHistory()
    {
        foreach (int history in new[] { 0, 1, 32 })
        {
            var device = new PagedTestDevice(); var log = new PagedCommitLog(device);
            for (int i = 0; i < history; i++) { log.Append(new string('p', 240)); log.Flush(); }
            var before = device.Live.Copy(); device.Calls.Clear(); log.Append("new 雪"); log.Flush();
            Assert.Equal(history + 1, device.Live.Pages.Count);
            for (int i = 0; i < history; i++) Assert.Equal(before.Pages[i], device.Live.Pages[i]);
            Assert.Equal(new[] { "writePage", "syncData", "writeMetadata" + (history + 1) % 2, "syncMetadata" + (history + 1) % 2 }, device.Calls);
        }
    }
    [Fact]
    public void RepeatedInitializationAndTailRepairFailuresKeepDurableDecisions()
    {
        for (int at = 1; at <= 3; at++)
            foreach (bool after in new[] { false, true })
            {
                var device = new PagedTestDevice { Fault = (at, after) }; Assert.Throws<IOException>(() => new PagedCommitLog(device));
                var recovered = device.Crash(); var log = new PagedCommitLog(recovered); Assert.Empty(log.Read()); log.Append("following"); log.Flush();
                Assert.Equal(new[] { "following" }, new PagedCommitLog(recovered.Crash()).Read());
            }
        var (image, prior) = Fixture("oneGroup"); image.Pages.Add(new byte[4096]); var probe = From(image); _ = new PagedCommitLog(probe);
        for (int offset = 0; offset < probe.Calls.Count; offset++)
        {
            if (probe.Calls[offset] == "readPage") continue;
            foreach (bool after in new[] { false, true })
            {
                var device = From(image); device.Fault = (offset + 1, after); Assert.Throws<IOException>(() => new PagedCommitLog(device));
                var recovered = device.Crash(); Assert.Equal(prior, new PagedCommitLog(recovered).Read()); Assert.Single(recovered.Durable.Pages);
            }
        }
    }
}
