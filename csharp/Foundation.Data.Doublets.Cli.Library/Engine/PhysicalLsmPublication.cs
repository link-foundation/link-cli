using System.Globalization;
using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Engine;

public sealed partial class PhysicalLsmJournalStore
{
    private sealed record Plan(bool Merge, SortedJournalEntry[] Rows, int StoredBytes, ulong Id, LsmWalState Reserved, string Reservation, LsmWalState Published);
    private static object?[] Wire(SortedJournalEntry row) => new object?[] { row.Key, row.Generation.ToString(CultureInfo.InvariantCulture), row.Entries };
    private void Replace(string record)
    { _ = Effect(() => _wal.Replace(new[] { record })); _recordCount = 1; _walBytes = Utf8.GetByteCount(record); }
    private void Commit(IReadOnlyList<SortedJournalEntry> changes)
    {
        if (_state.Generation == ulong.MaxValue) throw Invalid("generation exhausted");
        ulong generation = _state.Generation + 1;
        var rows = changes.Select(row => row with { Generation = generation }).OrderBy(row => row.Key, StringComparer.Ordinal).ToArray();
        var candidate = new SortedDictionary<string, Index>(_index, StringComparer.Ordinal);
        foreach (var row in rows) candidate[row.Key] = Describe(row, 0); Admit(candidate);
        if (rows.Length > _limits.BufferKeys || rows.Sum(Cost) > _limits.BufferBytes) throw Invalid("group exceeds buffer admission");
        string record = JsonSerializer.Serialize(new object[] { "put", generation.ToString(CultureInfo.InvariantCulture), rows.Select(Wire).ToArray() }); int recordBytes = Utf8.GetByteCount(record);
        if (_limits.MaxWalRecords < 2) throw Invalid("writes require a checkpoint and group");
        var merged = new SortedDictionary<string, SortedJournalEntry>(StringComparer.Ordinal);
        foreach (var row in _state.Buffer) merged.Add(row.Key, row); foreach (var row in rows) merged[row.Key] = row;
        Plan? plan = merged.Count > _limits.BufferKeys || merged.Values.Sum(Cost) > _limits.BufferBytes ? Prepare(false) : null;
        var basis = plan?.Published ?? _state;
        if (plan is not null) { merged.Clear(); foreach (var row in rows) merged.Add(row.Key, row); }
        if ((long)Utf8.GetByteCount(LsmWal.Encode(basis, _limits)) + recordBytes > _limits.MaxWalBytes) throw Invalid("checkpoint and following group exceed WAL admission");
        _ = LsmWal.Encode(basis with { Generation = generation, Buffer = merged.Values.ToArray() }, _limits);
        if (plan is not null) Publish(plan);
        var after = _state with { Generation = generation, Buffer = merged.Values.ToArray() };
        if (_recordCount + 1 > _limits.MaxWalRecords || _walBytes + recordBytes > _limits.MaxWalBytes) Replace(LsmWal.Encode(_state, _limits));
        Effect(() => _wal.Append(record)); Effect(_wal.Flush); _state = after; _recordCount++; _walBytes += recordBytes;
        foreach (var row in rows) _index[row.Key] = Describe(row, 0);
    }
    private Plan? Prepare(bool compact)
    {
        if (_state.Buffer.Count == 0 && (!compact || _state.Manifest.Count <= 1)) return null;
        if (_state.LastReservedTableId == ulong.MaxValue) throw Invalid("table identity exhausted");
        bool merge = compact || _state.Manifest.Count >= _limits.MaxRuns;
        var rows = merge ? _index.Keys.Select(key => Value(key)!).ToArray() : _state.Buffer.ToArray();
        long stored = 15L + rows.Sum(row => (long)Utf8.GetByteCount(JsonSerializer.Serialize(Wire(row))));
        if (stored > 67_108_864 || _state.Manifest.Count + 1 > _maxNamespace) throw Invalid("table or namespace admission");
        ulong id = _state.LastReservedTableId + 1; var reserved = _state with { LastReservedTableId = id };
        string reservation = LsmWal.Encode(reserved, _limits);
        var stub = new LsmTableDescriptor(id, new string('0', 32), ulong.MaxValue, rows.Length, (int)stored);
        var manifest = (merge ? Array.Empty<LsmTableDescriptor>() : _state.Manifest).Append(stub).ToArray();
        var published = reserved with { CoveredGeneration = reserved.Generation, Manifest = manifest, Buffer = Array.Empty<SortedJournalEntry>() };
        _ = LsmWal.Encode(published, _limits); return new(merge, rows, (int)stored, id, reserved, reservation, published);
    }
    private void Publish(Plan plan)
    {
        Replace(plan.Reservation); _state = plan.Reserved;
        var device = Effect(() => _provider.Create(plan.Id)); _leases.Add(plan.Id, device); CheckDevice(device);
        var table = Effect(() => SortedJournalTable.Create(device, plan.Rows, TableOptions())); _tables.Add(plan.Id, table);
        if (table.Count != plan.Rows.Length || table.StoredBytes != plan.StoredBytes) throw Invalid("prepared table bounds changed");
        var descriptor = new LsmTableDescriptor(plan.Id, Convert.ToHexString(table.Identity).ToLowerInvariant(), table.Digest, table.Count, table.StoredBytes);
        var published = plan.Published with { Manifest = (plan.Merge ? Array.Empty<LsmTableDescriptor>() : plan.Reserved.Manifest).Append(descriptor).ToArray() };
        Replace(LsmWal.Encode(published, _limits)); var obsolete = plan.Merge ? _state.Manifest : Array.Empty<LsmTableDescriptor>(); _state = published;
        foreach (var row in plan.Rows) _index[row.Key] = Describe(row, plan.Id);
        foreach (var old in obsolete)
        {
            Effect(() => _provider.Close(_leases[old.Id])); _leases.Remove(old.Id); _tables.Remove(old.Id); Effect(() => _provider.Delete(old.Id));
        }
    }
    public void Flush() => Run(() => { var plan = Prepare(false); if (plan is not null) Publish(plan); return true; });
    public void Compact() => Run(() => { var plan = Prepare(true); if (plan is not null) Publish(plan); return true; });
}
