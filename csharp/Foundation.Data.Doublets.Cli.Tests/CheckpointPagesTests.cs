using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class CheckpointPagesTests
{
    private sealed class Trace { internal List<string> Calls = new(); internal (int At, bool After)? Fault; }
    private sealed class Image
    {
        internal byte[]?[] Metadata = new byte[]?[2]; internal List<byte[]> Pages = new();
        internal Image Copy() => new() { Metadata = Metadata.Select(b => b?.ToArray()).ToArray(), Pages = Pages.Select(b => b.ToArray()).ToList() };
    }
    private sealed class Device(Trace trace, string name, Image image) : IRepairPageDevice
    {
        internal Image Live = image.Copy(), Durable = image.Copy();
        public uint MetadataFlags { get; set; }
        public bool SupportsDurableFlush { get; set; } = true;
        internal byte[] Identity = Enumerable.Range(0, 16).Select(i => (byte)i).ToArray();
        public byte[] NewDatabaseId() => Action("newDatabaseId", () => Identity.ToArray());
        public long DataLength => Action("dataLength", () => Live.Pages.Count * 4096L);
        private T Action<T>(string operation, Func<T> body)
        {
            string label = name + "." + operation; trace.Calls.Add(label);
            bool fault = trace.Fault?.At == trace.Calls.Count;
            if (fault && !trace.Fault!.Value.After) throw new IOException("injected " + label);
            T value = body();
            if (fault)
            {
                if (operation.StartsWith("writeMetadata", StringComparison.Ordinal)) Durable.Metadata = Live.Metadata.Select(b => b?.ToArray()).ToArray();
                throw new IOException("injected " + label);
            }
            return value;
        }
        private void Action(string operation, Action body) => Action(operation, () => { body(); return true; });
        public byte[]? ReadMetadata(int slot) => Action("readMetadata" + slot, () => Live.Metadata[slot]?.ToArray());
        public void WriteMetadata(int slot, ReadOnlySpan<byte> bytes) { byte[] copy = bytes.ToArray(); Action("writeMetadata" + slot, () => { Live.Metadata[slot] = copy; }); }
        public void SyncMetadata(int slot) => Action("syncMetadata" + slot, () => { Durable.Metadata[slot] = Live.Metadata[slot]?.ToArray(); });
        public byte[] ReadPage(ulong index) => Action("readPage", () => Live.Pages[(int)index].ToArray());
        public void WritePage(ulong index, ReadOnlySpan<byte> bytes)
        { byte[] copy = bytes.ToArray(); Action("writePage", () => { Assert.Equal((ulong)Live.Pages.Count, index); Live.Pages.Add(copy); }); }
        public void SyncData() => Action("syncData", () => { Durable.Pages = Live.Pages.Select(b => b.ToArray()).ToList(); });
        public void TruncateData(long length) => Action("truncateData", () => { Live.Pages.RemoveRange((int)(length / 4096), Live.Pages.Count - (int)(length / 4096)); });
        public void RepairPage(ulong index, ReadOnlySpan<byte> bytes) { byte[] copy = bytes.ToArray(); Action("repairPage", () => { Live.Pages[(int)index] = copy; }); }
        public void CompleteRecovery() => Action("completeRecovery", () => { });
    }
    private static (Device[] Devices, Trace Trace) Devices(Image[]? images = null)
    {
        var trace = new Trace(); images ??= new[] { new Image(), new Image(), new Image() };
        var devices = images.Select((image, i) => new Device(trace, "device" + i, image)).ToArray();
        return (devices, trace);
    }
    private static CheckpointPagedCommitLog Open(Device[] devices) => new(devices[0], devices[1], devices[2], maxPages: 4);
    private static (CheckpointPagedCommitLog Log, Device[] Devices, Trace Trace) Setup(Image[]? images = null)
    {
        var (devices, trace) = Devices(images);
        return (new CheckpointPagedCommitLog(devices[0], devices[1], devices[2], maxPages: 4), devices, trace);
    }
    private static Image[] Durable(Device[] devices) => devices.Select(d => d.Durable.Copy()).ToArray();
    private static JsonDocument Vectors() => JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol/checkpoint-pages-v1.json")));

    [Fact]
    public void IndependentRootVectorsRepeatedSwapsAndFollowingGroups()
    {
        using var vectors = Vectors(); var (log, devices, _) = Setup();
        Assert.Equal(vectors.RootElement.GetProperty("decisions").GetProperty("0").GetString(), Convert.ToHexStringLower(devices[0].Live.Metadata[0]!));
        log.Append("old"); log.Flush();
        for (int epoch = 1; epoch <= 2; epoch++)
        {
            string[] replacement = { string.Concat(Enumerable.Repeat("雪", 700)), "\ufeff\0😀 " + epoch, "" };
            Assert.True(log.Replace(replacement)); Assert.Equal(replacement, log.Read());
            Assert.Equal(vectors.RootElement.GetProperty("decisions").GetProperty(epoch.ToString()).GetString(), Convert.ToHexStringLower(devices[0].Live.Metadata[epoch % 2]!));
            Assert.Equal(replacement, Setup(Durable(devices)).Log.Read());
            log.Append("following"); log.Flush(); Assert.Equal(replacement.Append("following"), log.Read());
        }
    }

    [Fact]
    public void EveryReplacementIoFailureFencesAndReopensCompleteOldOrNewRecords()
    {
        var start = Setup(); start.Log.Append("acknowledged"); start.Log.Flush(); var images = Durable(start.Devices);
        string[] replacement = { string.Concat(Enumerable.Repeat("雪", 700)), "\ufeff\0😀", "" };
        var success = Setup(images); success.Trace.Calls.Clear(); success.Log.Replace(replacement);
        int operations = success.Trace.Calls.Count; Assert.InRange(operations, 12, 40);
        for (int at = 1; at <= operations; at++) foreach (bool after in new[] { false, true })
        {
            var (log, devices, trace) = Setup(images); trace.Calls.Clear(); trace.Fault = (at, after);
            Assert.Contains("injected", Assert.Throws<IOException>(() => log.Replace(replacement)).Message);
            int count = trace.Calls.Count;
            Assert.Throws<IOException>(() => log.Read()); Assert.Throws<IOException>(() => log.Append("rejected"));
            Assert.Throws<IOException>(() => log.Flush()); Assert.Throws<IOException>(() => log.Replace(new[] { "rejected" }));
            Assert.Equal(count, trace.Calls.Count);
            var reopened = Setup(Durable(devices)); var found = reopened.Log.Read().ToArray();
            Assert.True(found.SequenceEqual(new[] { "acknowledged" }) || found.SequenceEqual(replacement));
            reopened.Log.Append("next"); reopened.Log.Flush(); Assert.Equal(found.Append("next"), Setup(Durable(reopened.Devices)).Log.Read());
        }
    }

    [Fact]
    public void InvalidPendingAndIdenticalReplacementsPreserveUsableLog()
    {
        var (log, _, trace) = Setup(); log.Append("old"); log.Flush(); trace.Calls.Clear();
        Assert.False(log.Replace(new[] { "old" })); Assert.All(trace.Calls, call => Assert.True(call.EndsWith(".readPage", StringComparison.Ordinal) || call.EndsWith(".completeRecovery", StringComparison.Ordinal)));
        foreach (string invalid in new[] { "a\nb", "\r", "\ud800", new string('x', 10000) }) Assert.ThrowsAny<Exception>(() => log.Replace(new[] { invalid }));
        Assert.Equal(new[] { "old" }, log.Read()); log.Append("pending");
        Assert.Contains("pending", Assert.Throws<IOException>(() => log.Replace(new[] { "new" })).Message);
        log.Flush(); Assert.Equal(new[] { "old", "pending" }, log.Read());
        Assert.True(log.Replace(Array.Empty<string>())); Assert.Empty(log.Read()); Assert.True(log.SupportsDurableFlush);
    }

    [Fact]
    public void MalformedRootsStaleSelectedBanksAndUnrootedJournalsNeverTriggerWrites()
    {
        using var vectors = Vectors(); var data = vectors.RootElement;
        var start = Setup(); start.Log.Append("old"); start.Log.Flush(); start.Log.Replace(new[] { "new" });
        var source = Durable(start.Devices); var invalid = new List<Image[]>();
        void Add(Action<Image[]> change) { var image = source.Select(i => i.Copy()).ToArray(); change(image); invalid.Add(image); }
        foreach (var root in data.GetProperty("invalid").EnumerateArray()) Add(image => { image[0].Metadata[0] = Convert.FromHexString(root.GetString()!); });
        foreach (string root in new[] { data.GetProperty("decisions").GetProperty("3").GetString()!, data.GetProperty("foreignDecision").GetString()! }) Add(image => { image[0].Metadata[1] = Convert.FromHexString(root); });
        Add(image => { Array.Reverse(image[0].Metadata); });
        Add(image => { image[0].Pages.Add(new byte[4096]); });
        Add(image => { image[2].Metadata = new byte[]?[2]; });
        Add(image => { image[2].Metadata = image[1].Copy().Metadata; });
        Add(image => { image[2].Pages[0][56] ^= 1; });
        Add(image => { image[0].Metadata = new byte[]?[2]; });
        Add(image => { image[0].Metadata = new byte[]?[2]; image[1].Pages.Clear(); image[2].Pages.Clear(); });
        foreach (var image in invalid)
        {
            var (devices, trace) = Devices(image); Assert.ThrowsAny<Exception>(() => Open(devices));
            Assert.All(trace.Calls, call => Assert.True(call.Contains(".read", StringComparison.Ordinal) || call.EndsWith(".dataLength", StringComparison.Ordinal)));
            for (int i = 0; i < 3; i++) { Assert.Equal(image[i].Metadata, devices[i].Live.Metadata); Assert.Equal(image[i].Pages, devices[i].Live.Pages); }
        }
    }

    [Fact]
    public void EveryTornRootLengthAndDamagedChecksumRecoversPriorAcknowledgement()
    {
        var start = Setup(); start.Log.Append("old"); start.Log.Flush(); start.Log.Replace(new[] { "new" }); var source = Durable(start.Devices);
        for (int length = 0; length < 128; length++)
        {
            var image = source.Select(i => i.Copy()).ToArray(); image[0].Metadata[1] = image[0].Metadata[1]![..length];
            var (log, _, _) = Setup(image); Assert.Equal(0UL, log.Epoch); Assert.Equal(new[] { "old" }, log.Read());
        }
        source[0].Metadata[1]![120] ^= 1; Assert.Equal(new[] { "old" }, Setup(source).Log.Read());
    }

    [Fact]
    public void ExactU64EpochsAndExhaustionDoNotLoseFollowingGroups()
    {
        using var vectors = Vectors(); var data = vectors.RootElement;
        foreach (ulong epoch in new[] { 9007199254740993UL, ulong.MaxValue })
        {
            var image = new[] { new Image(), new Image(), new Image() };
            image[0].Metadata[1] = Convert.FromHexString(data.GetProperty("decisions").GetProperty(epoch.ToString()).GetString()!);
            image[2].Metadata[0] = Convert.FromHexString(data.GetProperty("emptyBanks").GetProperty(epoch.ToString()).GetString()!);
            var (log, devices, trace) = Setup(image); Assert.Equal(epoch, log.Epoch); Assert.False(log.Replace(Array.Empty<string>())); trace.Calls.Clear();
            if (epoch == ulong.MaxValue)
            {
                Assert.Contains("exhausted", Assert.Throws<IOException>(() => log.Replace(new[] { "new" })).Message);
                Assert.All(trace.Calls, call => Assert.EndsWith(".completeRecovery", call));
                log.Append("following"); log.Flush();
            }
            else { Assert.True(log.Replace(new[] { "new" })); Assert.Equal(epoch + 1, log.Epoch); }
            Assert.Equal(new[] { epoch == ulong.MaxValue ? "following" : "new" }, Setup(Durable(devices)).Log.Read());
        }
    }

    [Fact]
    public void DeviceModesLimitsIdentityAndDurabilityAreValidated()
    {
        foreach (uint[] flags in new[] { new uint[] { 1, 0, 0 }, new uint[] { 0, 2, 2 }, new uint[] { 0, 0, 1 } })
        {
            var (devices, trace) = Devices(); for (int i = 0; i < 3; i++) devices[i].MetadataFlags = flags[i];
            Assert.Throws<IOException>(() => Open(devices)); Assert.Empty(trace.Calls);
        }
        var initial = Devices(); var d = initial.Devices;
        Assert.Throws<ArgumentNullException>(() => new CheckpointPagedCommitLog(null!, d[1], d[2]));
        Assert.Throws<ArgumentNullException>(() => new CheckpointPagedCommitLog(d[0], null!, d[2]));
        Assert.Throws<ArgumentNullException>(() => new CheckpointPagedCommitLog(d[0], d[1], null!));
        foreach (var duplicate in new[] { new[] { d[0], d[0], d[2] }, new[] { d[0], d[1], d[0] }, new[] { d[0], d[1], d[1] } })
            Assert.Throws<ArgumentException>(() => new CheckpointPagedCommitLog(duplicate[0], duplicate[1], duplicate[2]));
        foreach (ulong max in new[] { 0UL, ulong.MaxValue }) Assert.Throws<ArgumentOutOfRangeException>(() => new CheckpointPagedCommitLog(d[0], d[1], d[2], max));
        Assert.Empty(initial.Trace.Calls);
        var identity = Devices(); identity.Devices[0].Identity = new byte[15]; Assert.Throws<IOException>(() => Open(identity.Devices));
        for (int disabled = 0; disabled < 3; disabled++) { var devices = Devices().Devices; devices[disabled].SupportsDurableFlush = false; Assert.False(Open(devices).SupportsDurableFlush); }
        var log = Setup().Log; Assert.True(log.SupportsReplacement);
        Assert.Throws<ArgumentNullException>(() => log.Replace(null!)); Assert.Throws<ArgumentNullException>(() => log.Replace(new[] { (string)null! }));
        Assert.Throws<ArgumentException>(() => log.Append("a\nb")); log.Append("still usable"); log.Flush();
    }

    [Fact]
    public void EveryInitializationFailureReopensEmptyAndAcceptsFollowingGroup()
    {
        int operations = Setup().Trace.Calls.Count; Assert.InRange(operations, 20, 40);
        for (int at = 1; at <= operations; at++) foreach (bool after in new[] { false, true })
        {
            var (devices, trace) = Devices(); trace.Fault = (at, after); Assert.Throws<IOException>(() => Open(devices));
            var recovered = Setup(Durable(devices)); Assert.Empty(recovered.Log.Read());
            recovered.Log.Append("following"); recovered.Log.Flush(); Assert.Equal(new[] { "following" }, Setup(Durable(recovered.Devices)).Log.Read());
        }
    }

    [Fact]
    public void OrdinaryReadAndFlushFailuresFenceAllLaterOperations()
    {
        foreach (bool flush in new[] { false, true })
        {
            var (log, _, trace) = Setup(); log.Append("old"); log.Flush(); if (flush) log.Append("pending"); trace.Calls.Clear(); trace.Fault = (1, false);
            Assert.Throws<IOException>(() => { if (flush) log.Flush(); else log.Read(); });
            Assert.Throws<IOException>(() => log.Read()); Assert.Throws<IOException>(() => log.Append("rejected"));
        }
    }

    [Fact]
    public void MirroredBanksRepairSelectedEpochBeforeRepeatedSwaps()
    {
        static CheckpointPagedCommitLog Mirrored(Device[] d) => new(d[0], new MirroredPageDevice(d[1], d[2]), new MirroredPageDevice(d[3], d[4]));
        var initial = Devices(Enumerable.Range(0, 5).Select(_ => new Image()).ToArray()); var log = Mirrored(initial.Devices);
        log.Append("old"); log.Flush(); log.Replace(new[] { "new" }); using var vectors = Vectors();
        Assert.Equal(vectors.RootElement.GetProperty("mirroredDecision").GetString(), Convert.ToHexStringLower(initial.Devices[0].Live.Metadata[1]!));
        var image = Durable(initial.Devices); image[3].Pages[0][56] ^= 1; image[3].Metadata[0] = new byte[17];
        var (devices, _) = Devices(image); log = Mirrored(devices); Assert.Equal(new[] { "new" }, log.Read());
        Assert.Equal(devices[3].Durable.Pages, devices[4].Durable.Pages); Assert.Equal(devices[3].Durable.Metadata, devices[4].Durable.Metadata);
        Assert.True(log.Replace(new[] { "third" })); Assert.True(log.Replace(new[] { "fourth" })); Assert.Equal(new[] { "fourth" }, log.Read());
    }

    [Fact]
    public void InterruptedMirroredReplacementReopensOldOrNewThenReusesPartialInactiveBank()
    {
        static CheckpointPagedCommitLog Mirrored(Device[] d) => new(d[0], new MirroredPageDevice(d[1], d[2]), new MirroredPageDevice(d[3], d[4]), maxPages: 4);
        var initial = Devices(Enumerable.Range(0, 5).Select(_ => new Image()).ToArray()); var original = Mirrored(initial.Devices);
        original.Append("old"); original.Flush(); var images = Durable(initial.Devices);
        string[] replacement = { string.Concat(Enumerable.Repeat("雪", 700)), "\ufeff\0😀" };
        var probe = Devices(images); var success = Mirrored(probe.Devices); probe.Trace.Calls.Clear(); success.Replace(replacement);
        int operations = probe.Trace.Calls.Count; Assert.InRange(operations, 30, 90);
        for (int at = 1; at <= operations; at++) foreach (bool after in new[] { false, true })
        {
            var (devices, trace) = Devices(images); var log = Mirrored(devices); trace.Calls.Clear(); trace.Fault = (at, after);
            try { log.Replace(replacement); }
            catch (IOException error)
            {
                Assert.Contains("injected", error.Message); int count = trace.Calls.Count;
                Assert.Throws<IOException>(() => log.Read()); Assert.Throws<IOException>(() => log.Replace(new[] { "rejected" })); Assert.Equal(count, trace.Calls.Count);
            }
            var recovered = Devices(Durable(devices)); var reopened = Mirrored(recovered.Devices); var found = reopened.Read().ToArray();
            Assert.True(found.SequenceEqual(new[] { "old" }) || found.SequenceEqual(replacement));
            reopened.Replace(new[] { "after recovery" }); Assert.Equal(new[] { "after recovery" }, Mirrored(Devices(Durable(recovered.Devices)).Devices).Read());
        }
    }
}
