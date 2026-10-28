namespace Foundation.Data.Doublets.Cli.Engine;

public sealed partial class PhysicalLsmJournalStore
{
    private sealed class DeferredRepair(IPageDevice device) : IPageDevice
    {
        public uint MetadataFlags => device.MetadataFlags;
        public bool SupportsDurableFlush => device.SupportsDurableFlush;
        public byte[] NewDatabaseId() => device.NewDatabaseId();
        public long DataLength => device.DataLength;
        public byte[]? ReadMetadata(int slot) => device.ReadMetadata(slot);
        public void WriteMetadata(int slot, ReadOnlySpan<byte> bytes) => device.WriteMetadata(slot, bytes);
        public void SyncMetadata(int slot) => device.SyncMetadata(slot);
        public byte[] ReadPage(ulong index) => device.ReadPage(index);
        public byte[] ReadCheckedPage(ulong index, ulong expected) => device.ReadCheckedPage(index, expected);
        public void WritePage(ulong index, ReadOnlySpan<byte> bytes) => device.WritePage(index, bytes);
        public void SyncData() => device.SyncData();
        public void TruncateData(long length) => device.TruncateData(length);
        public void CompleteRecovery() { }
    }
    private void Recover()
    {
        var names = _provider.List(_maxNamespace); ulong last = 0;
        if (names.Count > _maxNamespace) throw Invalid("incomplete namespace");
        foreach (ulong id in names) { if (id <= last) throw Invalid("unordered namespace"); last = id; }
        var records = _wal.Read();
        if (records.Count == 0)
        {
            if (names.Count != 0) throw Invalid("uninitialized namespace contains tables"); Replace(LsmWal.Encode(_state, _limits)); return;
        }
        _state = LsmWal.Recover(records, _limits); _recordCount = records.Count; _walBytes = records.Sum(record => (long)Utf8.GetByteCount(record));
        var selected = _state.Manifest.Select(item => item.Id).ToHashSet();
        if (names.Any(id => id > _state.LastReservedTableId) || selected.Except(names).Any()) throw Invalid("missing selected or unreserved table");
        var seen = new Dictionary<(string, ulong), ulong>();
        foreach (var descriptor in _state.Manifest)
        {
            var device = _provider.Open(descriptor.Id); _leases.Add(descriptor.Id, device); CheckDevice(device);
            var table = SortedJournalTable.Open(new DeferredRepair(device), TableOptions()); _tables.Add(descriptor.Id, table);
            if (Convert.ToHexString(table.Identity).ToLowerInvariant() != descriptor.Identity || table.Digest != descriptor.Digest
                || table.Count != descriptor.Count || table.StoredBytes != descriptor.StoredBytes) throw Invalid("selected table binding changed");
            foreach (var row in table.Entries())
            {
                if (row.Generation > _state.CoveredGeneration) throw Invalid("table generation exceeds coverage");
                if (seen.TryGetValue((row.Key, row.Generation), out ulong previous))
                {
                    var old = _tables[previous].Get(row.Key);
                    if (old is null || old.Generation != row.Generation || !Equal(old.Entries, row.Entries)) throw Invalid("conflicting table versions");
                }
                else seen.Add((row.Key, row.Generation), descriptor.Id);
                if (!_index.TryGetValue(row.Key, out var before) || row.Generation > before.Generation) _index[row.Key] = Describe(row, descriptor.Id);
            }
        }
        foreach (var row in _state.Buffer) _index[row.Key] = Describe(row, 0);
        Admit(_index);
        if (_index.Values.Select(row => row.Generation).DefaultIfEmpty(0UL).Max() != _state.Generation) throw Invalid("selected data is missing the global generation");
        foreach (var device in _leases.Values) Effect(device.CompleteRecovery);
        foreach (ulong id in names) if (!selected.Contains(id)) Effect(() => _provider.Delete(id));
    }
}
