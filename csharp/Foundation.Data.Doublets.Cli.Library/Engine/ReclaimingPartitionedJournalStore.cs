using System.Globalization;
namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Explicit epoch-fenced reclamation for dedicated new namespaces.
/// Providers remain caller-owned. No background collection runs automatically.</summary>
public sealed partial class ReclaimingPartitionedJournalStore : IAtomicJournalStore
{
    private readonly IJournalMaintenanceStore _catalog;
    private readonly Dictionary<string, IJournalMaintenanceStore> _partitions;
    private readonly Func<string, string> _route;
    private readonly ReclamationOptions _options;
    private readonly ImmutableObjectCache? _cache;
    private readonly JournalChunkOptions? _chunks;
    private sealed record Snapshot(JournalVersion[] Values, ReclamationCodec.Pointer?[] Pointers, JournalVersion Root, ulong Epoch);
    private sealed record Group(JournalVersion[]? Values, int Missing = -1);
    private ReclaimingPartitionedJournalStore(IJournalMaintenanceStore catalog, Dictionary<string, IJournalMaintenanceStore> partitions, Func<string, string> route, ReclamationOptions options, JournalChunkOptions? chunks)
    {
        _catalog = catalog; _partitions = partitions; _route = route; _options = options;
        _cache = options.CacheBytes > 0 ? new(options.CacheBytes) : null;
        _chunks = chunks;
    }
    public static ReclaimingPartitionedJournalStore Open(IJournalMaintenanceStore catalog, IReadOnlyDictionary<string, IJournalMaintenanceStore> partitions, Func<string, string> route, ReclamationOptions? options = null)
        => Open(catalog, partitions, route, options, null);
    /// <summary>Select bounded chunks for new writes. Readers accept blob2 and
    /// chunks2 roots; every staged object uses the captured catalog epoch.</summary>
    public static ReclaimingPartitionedJournalStore Open(IJournalMaintenanceStore catalog, IReadOnlyDictionary<string, IJournalMaintenanceStore> partitions, Func<string, string> route, ReclamationOptions? options, JournalChunkOptions? chunks)
    {
        if (chunks is not null) JournalChunks.Validate(chunks);
        options ??= new();
        if (options.MaxKeys <= 0 || options.MaxBytes <= 0 || options.PageKeys is < 1 or > 256 || options.CacheBytes < 0) throw new ArgumentOutOfRangeException(nameof(options));
        ArgumentNullException.ThrowIfNull(catalog); ArgumentNullException.ThrowIfNull(partitions); ArgumentNullException.ThrowIfNull(route);
        if (partitions.Count == 0) throw new ArgumentException("Provide named maintenance-capable data stores.", nameof(partitions));
        var owned = new Dictionary<string, IJournalMaintenanceStore>(StringComparer.Ordinal);
        foreach (var (name, provider) in partitions.OrderBy(item => item.Key, StringComparer.Ordinal))
        { ClusterEngine<uint>.ValidateKeys(new[] { name }); ArgumentNullException.ThrowIfNull(provider); owned.Add(name, provider); }
        var initial = ReclamationCodec.Scan(catalog, options, false);
        if (initial.Length == 0)
        {
            foreach (var provider in owned.Values)
                if (ReclamationCodec.Scan(provider, options, true).Length != 0) throw new InvalidDataException("Initial reclamation namespaces must be empty.");
            // One initialization attempt; unknown outcomes propagate without replay.
            catalog.CompareExchange(new[] { ReclamationCodec.EpochKey }, new[] { new JournalVersion(0, Array.Empty<string>()) }, new IReadOnlyList<string>?[] { new[] { "gc1", "0" } });
            initial = ReclamationCodec.Scan(catalog, options, false);
        }
        ReclamationCodec.Catalog(initial, owned);
        return new(catalog, owned, route, options, chunks);
    }
    public bool SupportsDurableFlush => _catalog.SupportsDurableFlush && _partitions.Values.All(provider => provider.SupportsDurableFlush);
    private Snapshot ReadSnapshot(string[] keys)
    {
        var values = ReclamationCodec.Read(_catalog, keys.Append(ReclamationCodec.EpochKey).ToArray());
        var root = values[^1]; var epoch = ReclamationCodec.Epoch(root);
        var shards = values[..^1];
        return new(shards, shards.Select(value => ReclamationCodec.ParsePointer(value, epoch, _partitions)).ToArray(), root, epoch);
    }
    private Group ReadGroup(Snapshot snapshot)
    {
        var result = new List<JournalVersion>();
        for (var index = 0; index < snapshot.Values.Length; index++)
        {
            var item = snapshot.Values[index]; var pointer = snapshot.Pointers[index];
            if (pointer is null) { result.Add(new(0, Array.Empty<string>())); continue; }
            var entries = new List<string>();
            foreach (var reference in pointer.References)
            {
                if (ReadObject(pointer.Partition, reference) is not { } data) return new(null, index);
                entries.AddRange(data);
            }
            result.Add(new(item.Generation, entries.ToArray()));
        }
        return new(result.ToArray());
    }
    public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys)
    {
        var selected = ReclamationCodec.Keys(keys); var first = ReadSnapshot(selected); var result = ReadGroup(first);
        if (result.Missing < 0) return result.Values!;
        var fresh = ReadSnapshot(selected); var a = first.Values[result.Missing]; var b = fresh.Values[result.Missing];
        if (a.Generation == b.Generation || a.Entries.SequenceEqual(b.Entries)) throw new InvalidDataException("Missing or modified referenced reclamation object.");
        // Discard all first-group values, including already owned/cache-hit entries.
        result = ReadGroup(fresh);
        if (result.Missing >= 0) throw new InvalidDataException("Reclamation object retired during bounded retry.");
        return result.Values!;
    }
    public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
    {
        var selected = ReclamationCodec.Keys(keys);
        if (expected.Count != selected.Length || updates.Count != selected.Length) throw new ArgumentException("Expected generations and updates must match shards.", nameof(updates));
        var guards = expected.Select(item => new JournalVersion(item.Generation, Array.Empty<string>())).ToArray();
        var staged = selected.Select((key, index) =>
        {
            if (updates[index] is not { } records) return (Data: (string[]?)null, Partition: "", Chunks: Array.Empty<IReadOnlyList<string>>());
            var data = ReclamationCodec.Records(records); var partition = _route(key);
            if (!_partitions.ContainsKey(partition)) throw new ArgumentException("Route must select a configured partition.", nameof(keys));
            var chunks = _chunks is null ? new IReadOnlyList<string>[] { data } : JournalChunks.Split(data, _chunks);
            return (Data: data, Partition: partition, Chunks: chunks);
        }).ToArray();
        var current = ReadSnapshot(selected);
        if (current.Values.Where((item, index) => item.Generation != guards[index].Generation).Any()) return false;
        var pointers = new List<IReadOnlyList<string>?>();
        foreach (var (data, partition, chunks) in staged)
        {
            if (data is null) { pointers.Add(null); continue; }
            var pointer = _chunks is null ? new List<string> { "blob2", partition }
                : new List<string> { "chunks2", partition, data.Length.ToString(CultureInfo.InvariantCulture), chunks.Length.ToString(CultureInfo.InvariantCulture) };
            foreach (var records in chunks)
            {
                var (key, generation) = StageObject(partition, current.Epoch, records);
                pointer.Add(key); pointer.Add(generation.ToString(CultureInfo.InvariantCulture));
                if (_chunks is not null) pointer.Add(records.Count.ToString(CultureInfo.InvariantCulture));
            }
            pointers.Add(pointer.ToArray());
        }
        return _catalog.CompareExchange(selected.Append(ReclamationCodec.EpochKey).ToArray(), guards.Append(current.Root).ToArray(), pointers.Append(null).ToArray());
    }
}
