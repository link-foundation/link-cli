using System.Collections;
using Foundation.Data.Doublets.Cli.Engine;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>Behaviors of small engine seams that scenario suites do not reach.</summary>
public sealed class EngineCoverageContractsTests
{
    private sealed class RecordingDevice : IPageDevice
    {
        public readonly List<string> Calls = new();
        public uint MetadataFlags { get { Calls.Add("flags"); return 7; } }
        public bool SupportsDurableFlush { get { Calls.Add("durable"); return true; } }
        public byte[] NewDatabaseId() { Calls.Add("id"); return [1]; }
        public long DataLength { get { Calls.Add("length"); return 11; } }
        public byte[]? ReadMetadata(int slot) { Calls.Add($"read-meta:{slot}"); return [2]; }
        public void WriteMetadata(int slot, ReadOnlySpan<byte> bytes) => Calls.Add($"write-meta:{slot}:{bytes.Length}");
        public void SyncMetadata(int slot) => Calls.Add($"sync-meta:{slot}");
        public byte[] ReadPage(ulong index) { Calls.Add($"read:{index}"); return [3]; }
        public byte[] ReadCheckedPage(ulong index, ulong expected) { Calls.Add($"checked:{index}:{expected}"); return [4]; }
        public void WritePage(ulong index, ReadOnlySpan<byte> bytes) => Calls.Add($"write:{index}:{bytes.Length}");
        public void SyncData() => Calls.Add("sync");
        public void TruncateData(long length) => Calls.Add($"truncate:{length}");
        public void CompleteRecovery() => Calls.Add("recovered");
    }

    [Fact]
    public void DeferredRepairForwardsEveryDeviceCallButDoesNotCompleteRecovery()
    {
        var inner = new RecordingDevice();
        IPageDevice device = new PhysicalLsmJournalStore.DeferredRepair(inner);
        Assert.Equal(7u, device.MetadataFlags);
        Assert.True(device.SupportsDurableFlush);
        Assert.Equal(new byte[] { 1 }, device.NewDatabaseId());
        Assert.Equal(11, device.DataLength);
        Assert.Equal(new byte[] { 2 }, device.ReadMetadata(1));
        device.WriteMetadata(1, new byte[2]);
        device.SyncMetadata(1);
        Assert.Equal(new byte[] { 3 }, device.ReadPage(5));
        Assert.Equal(new byte[] { 4 }, device.ReadCheckedPage(5, 9));
        device.WritePage(5, new byte[3]);
        device.SyncData();
        device.TruncateData(4);
        device.CompleteRecovery();
        Assert.Equal(new[]
        {
            "flags", "durable", "id", "length", "read-meta:1", "write-meta:1:2", "sync-meta:1", "read:5",
            "checked:5:9", "write:5:3", "sync", "truncate:4",
        }, inner.Calls);
    }

    [Fact]
    public void DirectorySyncReportsEachFailureAndClosesExactlyOnce()
    {
        var closes = 0;
        int Close(int result) { closes++; return result; }

        Assert.Contains("Cannot open", Assert.Throws<IOException>(() =>
            FilePageDevice.SyncDirectory("missing", (_, _) => -1, _ => 0, _ => Close(0))).Message);
        Assert.Equal(0, closes);

        FilePageDevice.SyncDirectory("present", (_, _) => 3, _ => 0, _ => Close(0));
        Assert.Equal(1, closes);

        Assert.Contains("Cannot synchronize", Assert.Throws<IOException>(() =>
            FilePageDevice.SyncDirectory("present", (_, _) => 3, _ => 1, _ => Close(0))).Message);
        Assert.Equal(2, closes);

        Assert.Contains("Cannot close", Assert.Throws<IOException>(() =>
            FilePageDevice.SyncDirectory("present", (_, _) => 3, _ => 0, _ => Close(1))).Message);
        Assert.Equal(3, closes);

        var both = Assert.Throws<AggregateException>(() =>
            FilePageDevice.SyncDirectory("present", (_, _) => 3, _ => 1, _ => Close(1)));
        Assert.Equal(2, both.InnerExceptions.Count);
        Assert.Equal(4, closes);
    }

    [Theory]
    [InlineData(StateStructure.Flat)]
    [InlineData(StateStructure.Shared)]
    [InlineData(StateStructure.SortedRuns)]
    public void RowsExposeTheSameViewForEveryStructure(StateStructure structure)
    {
        var rows = new EngineRows<uint>(structure);
        var row = new EngineRow<uint>(new Link<uint>(2, 0, 0), "two");
        rows.Set(2, row);
        Assert.Equal(row, rows[2]);
        Assert.Equal(new[] { 2u }, rows.Keys);
        var seen = new List<object?>();
        foreach (var entry in (IEnumerable)rows) seen.Add(entry);
        Assert.Equal(new object?[] { new KeyValuePair<uint, EngineRow<uint>>(2, row) }, seen);
    }

    [Fact]
    public void FileLsmCompareDeleteRemovesOnlyTheExpectedGeneration()
    {
        var directory = Directory.CreateTempSubdirectory("lsm-delete-").FullName;
        try
        {
            using var store = FileLsmJournalStore.Open(directory);
            Assert.True(store.CompareExchange(["a"], store.Read(["a"]), [["record"]]));
            var generation = store.Read(["a"])[0].Generation;
            Assert.False(store.CompareDelete("a", generation + 1));
            Assert.True(store.CompareDelete("a", generation));
            Assert.Empty(store.Read(["a"])[0].Entries);
            Assert.Empty(store.ScanNamespace());
            Assert.False(store.CompareDelete("a", generation));
        }
        finally { Directory.Delete(directory, true); }
    }

    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(2)]
    public void PreviewRefusesAnUpdateBeyondTheAddressLimit(int beyond)
    {
        UInt128 tooLarge = (UInt128)ulong.MaxValue + 1;
        UInt128 Part(int position) => position == beyond ? tooLarge : UInt128.One;
        var (id, source, target) = (Part(0), Part(1), Part(2));
        var rows = new[] { new EngineRow<UInt128>(new Link<UInt128>(id, 1, 1), null) };
        var constants = new LinksConstants<UInt128>((UInt128.One, UInt128.MaxValue));
        var preview = new QueryPreview<UInt128>(rows, 2, [], constants, limits: new QueryLimits());
        Assert.Contains("Unsupported stored query address", Assert.Throws<InvalidOperationException>(() =>
            preview.Update([id], [id, source, target], null)).Message);
    }
}
