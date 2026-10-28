using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

// Finite shared schedules span the real WAL, table devices and namespace.
internal sealed class PhysicalLsmHarness
{
    internal sealed record Snapshot(PagedTestDevice.Image[] Wal, Dictionary<ulong, PagedTestDevice.Image> Tables);
    internal readonly List<string> Trace = new();
    internal (int At, bool After)? Fault;
    internal readonly Device[] Devices;
    internal readonly Namespace Tables;
    internal PhysicalLsmHarness(Snapshot? image = null)
    {
        Devices = Enumerable.Range(0, 3).Select(i => new Device(this, new[] { "root", "bank0", "bank1" }[i], image?.Wal[i])).ToArray();
        Tables = new Namespace(this);
        if (image is not null) foreach (var (id, bytes) in image.Tables) Tables.Files.Add(id, new Device(this, "table-" + id, bytes));
    }
    internal T Effect<T>(string name, Func<T> action, Action? persistUncertain = null)
    {
        Trace.Add(name); bool fault = Fault?.At == Trace.Count;
        if (fault && !Fault!.Value.After) throw new IOException("injected " + name);
        T result = action();
        if (fault) { persistUncertain?.Invoke(); throw new IOException("injected " + name); }
        return result;
    }
    internal void Effect(string name, Action action, Action? persistUncertain = null) => Effect(name, () => { action(); return true; }, persistUncertain);
    internal void Reset() { Trace.Clear(); Fault = null; }
    internal Snapshot Image() => new(Devices.Select(d => d.Inner.Durable.Copy()).ToArray(), Tables.Files.ToDictionary(p => p.Key, p => p.Value.Inner.Durable.Copy()));
    internal CheckpointPagedCommitLog Wal() => new(Devices[0], Devices[1], Devices[2], 128);
    internal static LsmStoreOptions Options() => new(new LsmOptions(32, 4096, 2, 128, 2, 4, 8192), 16);
    internal PhysicalLsmJournalStore Open() => PhysicalLsmJournalStore.Open(Wal(), Tables, Options());
    internal sealed class Device : IRepairPageDevice
    {
        private readonly PhysicalLsmHarness _owner;
        internal readonly string Name;
        internal readonly PagedTestDevice Inner;
        internal Action? OnComplete;
        internal Device(PhysicalLsmHarness owner, string name, PagedTestDevice.Image? image = null)
        { _owner = owner; Name = name; Inner = image is null ? new() : PagedTestDevice.From(image); }
        public bool SupportsDurableFlush => Inner.SupportsDurableFlush;
        public uint MetadataFlags => Inner.MetadataFlags;
        public long DataLength => Inner.DataLength;
        public byte[] NewDatabaseId() => Inner.NewDatabaseId();
        public byte[]? ReadMetadata(int slot) => Inner.ReadMetadata(slot);
        public byte[] ReadPage(ulong index) => Inner.ReadPage(index);
        public void CompleteRecovery() { OnComplete?.Invoke(); Inner.CompleteRecovery(); }
        public void WritePage(ulong index, ReadOnlySpan<byte> bytes)
        { byte[] copy = bytes.ToArray(); _owner.Effect(Name + ":writePage", () => Inner.WritePage(index, copy)); }
        public void RepairPage(ulong index, ReadOnlySpan<byte> bytes)
        { byte[] copy = bytes.ToArray(); _owner.Effect(Name + ":repairPage", () => Inner.RepairPage(index, copy)); }
        public void SyncData() => _owner.Effect(Name + ":syncData", Inner.SyncData);
        public void WriteMetadata(int slot, ReadOnlySpan<byte> bytes)
        {
            byte[] copy = bytes.ToArray();
            _owner.Effect(Name + ":writeMetadata" + slot, () => Inner.WriteMetadata(slot, copy), () => Inner.Durable.Metadata = Inner.Live.Metadata.Select(p => p?.ToArray()).ToArray());
        }
        public void SyncMetadata(int slot) => _owner.Effect(Name + ":syncMetadata" + slot, () => Inner.SyncMetadata(slot));
        public void TruncateData(long length) => _owner.Effect(Name + ":truncateData", () => Inner.TruncateData(length));
    }
    internal sealed class Namespace : ILsmTableNamespace
    {
        private readonly PhysicalLsmHarness _owner;
        internal readonly SortedDictionary<ulong, Device> Files = new();
        internal readonly HashSet<IPageDevice> Leases = new();
        internal Namespace(PhysicalLsmHarness owner) { _owner = owner; }
        public bool SupportsDurableFlush => true;
        public IReadOnlyList<ulong> List(int maxEntries)
        { if (Files.Count > maxEntries) throw new InvalidDataException("namespace admission"); return Files.Keys.ToArray(); }
        public IPageDevice Create(ulong id)
        {
            Device? device = null;
            try
            {
                return _owner.Effect("create:" + id, () =>
                {
                    Assert.False(Files.ContainsKey(id)); device = new Device(_owner, "table-" + id); Files.Add(id, device); Assert.True(Leases.Add(device)); return device;
                });
            }
            catch { if (device is not null) Leases.Remove(device); throw; }
        }
        public IPageDevice Open(ulong id)
        { var device = Files[id]; Assert.True(Leases.Add(device)); return device; }
        public void Close(IPageDevice device) => _owner.Effect("close:" + ((Device)device).Name, () => { Leases.Remove(device); });
        public void Delete(ulong id) => _owner.Effect("delete:" + id, () => { Assert.DoesNotContain(Files[id], Leases); Assert.True(Files.Remove(id)); });
    }
    internal static void Same(Snapshot expected, Snapshot actual)
    {
        Assert.Equal(expected.Tables.Keys.Order(), actual.Tables.Keys.Order());
        for (int i = 0; i < 3; i++) PagedTestDevice.Same(expected.Wal[i], actual.Wal[i]);
        foreach (var (id, bytes) in expected.Tables) PagedTestDevice.Same(bytes, actual.Tables[id]);
    }
}
