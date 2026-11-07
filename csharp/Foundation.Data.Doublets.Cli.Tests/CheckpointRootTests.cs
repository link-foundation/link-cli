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
    private static IPageDevice[] RootBanks(Device[] devices, uint flags) => flags == 1
        ? new IPageDevice[] { new MirroredPageDevice(devices[1], devices[3]), new MirroredPageDevice(devices[2], devices[4]) }
        : new IPageDevice[] { devices[1], devices[2] };
    private static CheckpointPagedCommitLog RootOpen(Device[] devices, uint flags)
    {
        var banks = RootBanks(devices, flags);
        return new(new MirroredCheckpointRoot(devices[0], devices[5], flags), banks[0], banks[1], maxPages: 4);
    }
    private static (CheckpointPagedCommitLog Log, Device[] Devices, Trace Trace) RootSetup(Image[]? images = null, uint flags = 0)
    {
        var (devices, trace) = Devices(images ?? Enumerable.Range(0, 6).Select(_ => new Image()).ToArray());
        return (RootOpen(devices, flags), devices, trace);
    }
    private static System.Text.Json.JsonDocument RootVectors() => System.Text.Json.JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol/checkpoint-root-mirror-v1.json")));
    private static byte[] RootVector(System.Text.Json.JsonElement vectors, ulong epoch, uint flags) => Convert.FromHexString(vectors.GetProperty("decisions").EnumerateArray()
        .Single(v => v.GetProperty("epoch").GetString() == epoch.ToString(System.Globalization.CultureInfo.InvariantCulture) && v.GetProperty("bankFlags").GetUInt32() == flags).GetProperty("bytes").GetString()!);
    private static void RootUnchanged(Device[] devices, Image[] images)
    {
        for (int index = 0; index < images.Length; index++)
        { Assert.Equal(images[index].Metadata, devices[index].Live.Metadata); Assert.Equal(images[index].Pages, devices[index].Live.Pages); }
    }
    private static void RootFenced(MirroredCheckpointRoot root, Trace trace)
    {
        int before = trace.Calls.Count;
        foreach (Action action in new Action[] { () => root.NewDatabaseId(), () => { _ = root.DataLength; }, () => root.ReadMetadata(0), () => root.WriteMetadata(0, new byte[128]),
            () => root.SyncMetadata(0), () => root.SyncData(), () => root.TruncateData(0), () => root.ReadPage(0), () => root.WritePage(0, new byte[4096]), () => root.CompleteRecovery() })
            Assert.Contains("fenced", Assert.Throws<IOException>(action).Message);
        Assert.Equal(before, trace.Calls.Count);
    }

    [Fact]
    public void IndependentRootModeVectorsPreserveFullEpochsAndOwnedBytes()
    {
        using var vectors = RootVectors();
        foreach (var vector in vectors.RootElement.GetProperty("decisions").EnumerateArray())
        {
            var (devices, trace) = Devices(new[] { new Image(), new Image() });
            int slot = vector.GetProperty("slot").GetInt32(); byte[] bytes = Convert.FromHexString(vector.GetProperty("bytes").GetString()!);
            foreach (var device in devices) device.Live.Metadata[slot] = bytes.ToArray();
            var root = new MirroredCheckpointRoot(devices[0], devices[1], vector.GetProperty("bankFlags").GetUInt32());
            Assert.Equal(2U | vector.GetProperty("bankFlags").GetUInt32(), root.MetadataFlags); Assert.True(root.SupportsDurableFlush);
            var selected = root.ReadMetadata(slot)!; Assert.Equal(bytes, selected); selected[0] ^= 1; Assert.Equal(bytes, root.ReadMetadata(slot));
            trace.Calls.Clear(); root.CompleteRecovery(); Assert.Empty(trace.Calls);
            devices[1].SupportsDurableFlush = false; Assert.False(root.SupportsDurableFlush);
        }
    }

    [Fact]
    public void EveryRootByteTearPreservesAcknowledgedEpochAndFollowingGroups()
    {
        foreach (uint flags in new uint[] { 0, 1 })
        {
            var start = RootSetup(flags: flags); start.Log.Append("old"); start.Log.Flush(); start.Log.Replace(new[] { "acknowledged" }); var source = Durable(start.Devices);
            foreach (int side in new[] { 0, 5 }) for (int position = 0; position < 128; position++)
            {
                var image = source.Select(i => i.Copy()).ToArray(); image[side].Metadata[1]![position] ^= 1;
                var recovered = RootSetup(image, flags); Assert.Equal(1UL, recovered.Log.Epoch); Assert.Equal(new[] { "acknowledged" }, recovered.Log.Read());
                var repaired = Durable(recovered.Devices); Assert.Equal(repaired[0].Metadata, repaired[5].Metadata);
            }
            for (int epoch = 2; epoch <= 4; epoch++)
            {
                start.Log.Replace(new[] { "acknowledged" + epoch }); var image = Durable(start.Devices); image[epoch % 2 == 1 ? 0 : 5].Metadata[epoch % 2] = null;
                var recovered = RootSetup(image, flags); Assert.Equal((ulong)epoch, recovered.Log.Epoch);
                recovered.Log.Append("following"); recovered.Log.Flush(); recovered.Log.Replace(new[] { "next", "following" });
                Assert.Equal(new[] { "next", "following" }, RootSetup(Durable(recovered.Devices), flags).Log.Read());
            }
        }
    }

    [Fact]
    public void InvalidOrConflictingRootsAndSelectedBanksNeverTriggerRepair()
    {
        using var vectors = RootVectors(); var data = vectors.RootElement;
        foreach (uint flags in new uint[] { 0, 1 })
        {
            var start = RootSetup(flags: flags); start.Log.Append("old"); start.Log.Flush(); start.Log.Replace(new[] { "acknowledged" });
            var source = Durable(start.Devices); var invalid = new List<Image[]>();
            void Add(Action<Image[]> change) { var image = source.Select(i => i.Copy()).ToArray(); change(image); invalid.Add(image); }
            Add(i => { i[0].Metadata[1]![120] ^= 1; i[5].Metadata[1]![120] ^= 1; });
            Add(i => { i[5].Pages.Add(new byte[4096]); });
            Add(i => { i[0].Metadata[0] = RootVector(data, 4, flags); });
            Add(i => { i[0].Metadata[0] = RootVector(data, 4, flags); i[5].Metadata[0] = RootVector(data, 4, flags); });
            if (flags == 0) foreach (string key in new[] { "foreign", "wrongMode", "unknownMode", "unknownVersion", "nonzeroPadding" })
                Add(i => { i[0].Metadata[1] = Convert.FromHexString(data.GetProperty(key).GetString()!); });
            Add(i => { i[0].Metadata[1]![120] ^= 1; i[2].Pages[0][56] ^= 1; if (flags == 1) i[4].Pages[0][56] ^= 1; });
            Add(i => { i[0].Metadata[1]![120] ^= 1; i[2].Metadata = new byte[]?[2]; if (flags == 1) i[4].Metadata = new byte[]?[2]; });
            foreach (var image in invalid)
            {
                var (devices, trace) = Devices(image); Assert.ThrowsAny<Exception>(() => RootOpen(devices, flags));
                Assert.All(trace.Calls, call => Assert.DoesNotMatch("write|sync|truncate|repair", call)); RootUnchanged(devices, image);
            }
        }
    }

    [Fact]
    public void RootRepairRechecksEverySourceBeforeAnyWriteAndFencesChanges()
    {
        var start = RootSetup(); start.Log.Replace(new[] { "acknowledged" }); var images = Durable(start.Devices);
        images[0].Metadata[0]![120] ^= 1; images[5].Metadata[1]![120] ^= 1;
        var (devices, trace) = Devices(images); var root = new MirroredCheckpointRoot(devices[0], devices[5], 0);
        root.ReadMetadata(0); root.ReadMetadata(1); trace.Calls.Clear(); devices[0].Live.Metadata[1]![120] ^= 1;
        Assert.Contains("source changed", Assert.Throws<IOException>(root.CompleteRecovery).Message);
        Assert.All(trace.Calls, call => Assert.Contains("readMetadata", call)); RootFenced(root, trace);
    }

    [Fact]
    public void EveryPublicationCutRecoversOldOrNewAndPreservesAcknowledgements()
    {
        foreach (uint flags in new uint[] { 0, 1 })
        {
            var start = RootSetup(flags: flags); start.Log.Append("old"); start.Log.Flush(); var images = Durable(start.Devices);
            string[] replacement = { string.Concat(Enumerable.Repeat("雪", 700)), "\ufeff\0😀", "" };
            var success = RootSetup(images, flags); success.Trace.Calls.Clear(); success.Log.Replace(replacement);
            int operations = success.Trace.Calls.Count; Assert.InRange(operations, 18, 80);
            Assert.Equal(new[] { "device0.writeMetadata1", "device5.writeMetadata1", "device0.syncMetadata1", "device5.syncMetadata1" }, success.Trace.Calls.TakeLast(4));
            for (int at = 1; at <= operations; at++) foreach (bool after in new[] { false, true })
            {
                var current = RootSetup(images, flags); current.Trace.Calls.Clear(); current.Trace.Fault = (at, after); bool acknowledged = false;
                try { acknowledged = current.Log.Replace(replacement); }
                catch (IOException error)
                {
                    Assert.Contains("injected", error.Message); int count = current.Trace.Calls.Count;
                    Assert.Throws<IOException>(() => current.Log.Read()); Assert.Throws<IOException>(() => current.Log.Append("bad")); Assert.Throws<IOException>(() => current.Log.Flush()); Assert.Throws<IOException>(() => current.Log.Replace(new[] { "bad" }));
                    Assert.Equal(count, current.Trace.Calls.Count);
                }
                var recovered = RootSetup(Durable(current.Devices), flags); var found = recovered.Log.Read().ToArray();
                if (acknowledged) Assert.Equal(replacement, found); else Assert.True(found.SequenceEqual(new[] { "old" }) || found.SequenceEqual(replacement));
                recovered.Log.Append("following"); recovered.Log.Flush(); var following = found.Concat(new[] { "following", "compacted" }).ToArray(); recovered.Log.Replace(following);
                Assert.Equal(following, RootSetup(Durable(recovered.Devices), flags).Log.Read());
            }
        }
    }

    [Fact]
    public void EveryRootRepairCutPreservesAcknowledgedEpochAndBankReuse()
    {
        foreach (uint flags in new uint[] { 0, 1 }) foreach (int side in new[] { 0, 5 })
        {
            var start = RootSetup(flags: flags); start.Log.Append("old"); start.Log.Flush(); start.Log.Replace(new[] { "acknowledged" });
            var images = Durable(start.Devices); images[side].Metadata[1]![120] ^= 1;
            var success = RootSetup(images, flags); int operations = success.Trace.Calls.Count; Assert.InRange(operations, 12, 80);
            for (int at = 1; at <= operations; at++) foreach (bool after in new[] { false, true })
            {
                var attempt = Devices(images); attempt.Trace.Fault = (at, after);
                try { Assert.Equal(new[] { "acknowledged" }, RootOpen(attempt.Devices, flags).Read()); }
                catch (IOException error) { Assert.True(error.Message.Contains("injected", StringComparison.Ordinal) || error.Message.Contains("unreadable", StringComparison.Ordinal)); }
                var recovered = RootSetup(Durable(attempt.Devices), flags); Assert.Equal(1UL, recovered.Log.Epoch); Assert.Equal(new[] { "acknowledged" }, recovered.Log.Read());
                recovered.Log.Append("following"); recovered.Log.Flush(); recovered.Log.Replace(new[] { "reused", "following" });
                Assert.Equal(new[] { "reused", "following" }, RootSetup(Durable(recovered.Devices), flags).Log.Read());
            }
        }
    }

    [Fact]
    public void RootModeAndBankModeConversionsRejectBeforeMutation()
    {
        foreach (uint flags in new uint[] { 0, 1 })
        {
            var start = RootSetup(flags: flags); start.Log.Replace(new[] { "acknowledged" }); var images = Durable(start.Devices);
            var (devices, trace) = Devices(images); var banks = RootBanks(devices, flags);
            Assert.Throws<IOException>(() => new CheckpointPagedCommitLog(devices[0], banks[0], banks[1]));
            var wrong = new MirroredCheckpointRoot(devices[0], devices[5], 1 - flags);
            Assert.Throws<IOException>(() => new CheckpointPagedCommitLog(wrong, banks[0], banks[1]));
            RootUnchanged(devices, images); Assert.All(trace.Calls, call => Assert.DoesNotMatch("write|sync|truncate|repair", call));
        }
        var original = Setup(); original.Log.Replace(new[] { "old" }); var raw = Durable(original.Devices);
        var source = raw.Concat(new[] { new Image(), new Image(), new Image() }).ToArray(); var reopened = Devices(source).Devices;
        Assert.Throws<IOException>(() => RootOpen(reopened, 0)); RootUnchanged(reopened, source);
    }

    [Fact]
    public void RawRootContractRejectsBadModesSlotsPublicationsAndData()
    {
        var (devices, trace) = Devices(new[] { new Image(), new Image() });
        Assert.Throws<ArgumentNullException>(() => new MirroredCheckpointRoot(null!, devices[1], 0));
        Assert.Throws<ArgumentNullException>(() => new MirroredCheckpointRoot(devices[0], null!, 0));
        Assert.Throws<ArgumentException>(() => new MirroredCheckpointRoot(devices[0], devices[0], 0));
        Assert.Throws<ArgumentException>(() => new MirroredCheckpointRoot(devices[0], devices[1], 2));
        devices[1].MetadataFlags = 1; Assert.Throws<ArgumentException>(() => new MirroredCheckpointRoot(devices[0], devices[1], 0)); devices[1].MetadataFlags = 0;
        var root = new MirroredCheckpointRoot(devices[0], devices[1], 0); Assert.Equal(Enumerable.Range(0, 16).Select(i => (byte)i), root.NewDatabaseId()); trace.Calls.Clear();
        foreach (int slot in new[] { -1, 2 })
        { Assert.Throws<IOException>(() => root.ReadMetadata(slot)); Assert.Throws<IOException>(() => root.WriteMetadata(slot, new byte[128])); Assert.Throws<IOException>(() => root.SyncMetadata(slot)); }
        Assert.Throws<IOException>(() => root.WriteMetadata(0, new byte[128])); Assert.Throws<IOException>(() => root.ReadPage(0)); Assert.Throws<IOException>(() => root.WritePage(0, new byte[4096])); Assert.Throws<IOException>(() => root.TruncateData(1)); Assert.Empty(trace.Calls);
        root.TruncateData(0); Assert.Equal(new[] { "device0.truncateData", "device1.truncateData" }, trace.Calls);
        devices[0].Live.Pages.Add(new byte[4096]); Assert.Throws<IOException>(() => { _ = root.DataLength; }); devices[0].Live.Pages.Clear();
        devices[1].Live.Pages.Add(new byte[4096]); Assert.Throws<IOException>(() => { _ = root.DataLength; });
    }
    [Fact]
    public void IncompatibleEmptyRootAndBankModesRejectBeforeInitialization()
    {
        var (devices, trace) = Devices(Enumerable.Range(0, 6).Select(_ => new Image()).ToArray());
        var root = new MirroredCheckpointRoot(devices[0], devices[5], 1);
        Assert.Throws<IOException>(() => new CheckpointPagedCommitLog(root, devices[1], devices[2]));
        Assert.Empty(trace.Calls); RootUnchanged(devices, Enumerable.Range(0, 6).Select(_ => new Image()).ToArray());
    }
    [Fact]
    public void RawRootMutationFailuresFenceWithoutRetryAndEmptySelectionsClearRepairs()
    {
        using var vectors = RootVectors(); byte[] bytes = RootVector(vectors.RootElement, 0, 0);
        for (int method = 0; method < 4; method++) foreach (bool after in new[] { false, true })
        {
            var (devices, trace) = Devices(new[] { new Image(), new Image() }); var root = new MirroredCheckpointRoot(devices[0], devices[1], 0); trace.Fault = (1, after);
            Action action = method switch { 0 => root.SyncData, 1 => () => root.TruncateData(0), 2 => () => root.SyncMetadata(0), _ => () => root.WriteMetadata(0, bytes) };
            Assert.Contains("injected", Assert.Throws<IOException>(action).Message); RootFenced(root, trace);
        }
        var empty = Devices(new[] { new Image(), new Image() }); var cleared = new MirroredCheckpointRoot(empty.Devices[0], empty.Devices[1], 0);
        empty.Devices[0].Live.Metadata[0] = bytes.ToArray(); Assert.Equal(bytes, cleared.ReadMetadata(0));
        foreach (var device in empty.Devices) device.Live.Metadata[0] = Array.Empty<byte>();
        Assert.Null(cleared.ReadMetadata(0)); empty.Trace.Calls.Clear(); cleared.CompleteRecovery(); Assert.Empty(empty.Trace.Calls);
    }
}
