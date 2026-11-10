using System.Globalization;
using System.Text;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Immutable journal objects in independent stores, with small pointers
/// published atomically in a catalog. Stores are caller-owned. Object namespaces
/// must remain append-only; deleting or rewriting objects invalidates snapshots.</summary>
public sealed class PartitionedJournalStore : IAtomicJournalStore
{
    private readonly IAtomicJournalStore _catalog;
    private readonly Dictionary<string, IAtomicJournalStore> _partitions;
    private readonly Func<string, string> _route;
    private readonly ImmutableObjectCache? _cache;
    private readonly JournalChunkOptions? _chunks;
    public PartitionedJournalStore(IAtomicJournalStore catalog, IReadOnlyDictionary<string, IAtomicJournalStore> partitions, Func<string, string> route)
        : this(catalog, partitions, route, 0) { }
    /// <summary>Optional LRU payload budget: one byte per object
    /// and record plus record UTF-8 bytes; zero disables. Metadata and returned
    /// copies add memory. Catalog reads always stay fresh.</summary>
    public PartitionedJournalStore(IAtomicJournalStore catalog, IReadOnlyDictionary<string, IAtomicJournalStore> partitions, Func<string, string> route, long cacheBytes)
        : this(catalog, partitions, route, cacheBytes, null) { }
    /// <summary>Select bounded immutable chunks for new writes. Staging still
    /// reads and compares complete contents, including cached objects.</summary>
    public PartitionedJournalStore(IAtomicJournalStore catalog, IReadOnlyDictionary<string, IAtomicJournalStore> partitions, Func<string, string> route, long cacheBytes, JournalChunkOptions? chunks)
    {
        ArgumentNullException.ThrowIfNull(catalog); ArgumentNullException.ThrowIfNull(partitions); ArgumentNullException.ThrowIfNull(route);
        ArgumentOutOfRangeException.ThrowIfNegative(cacheBytes);
        if (chunks is not null) JournalChunks.Validate(chunks);
        if (partitions.Count == 0) throw new ArgumentException("Provide named atomic data stores.", nameof(partitions));
        _partitions = new(StringComparer.Ordinal);
        foreach (var (name, store) in partitions) { ClusterEngine<uint>.ValidateKeys(new[] { name }); ArgumentNullException.ThrowIfNull(store); _partitions.Add(name, store); }
        _catalog = catalog; _route = route;
        _cache = cacheBytes > 0 ? new(cacheBytes) : null;
        _chunks = chunks;
    }
    public bool SupportsDurableFlush => _catalog.SupportsDurableFlush && _partitions.Values.All(store => store.SupportsDurableFlush);
    // FNV indexes objects; full equality at creation rejects collisions safely.
    // This checksum does not authenticate an untrusted storage service.
    private static string Address(IReadOnlyList<string> records)
    {
        var hash = 0xcbf29ce484222325UL;
        void Add(byte[] bytes) { foreach (var value in bytes) hash = unchecked((hash ^ value) * 0x100000001b3UL); }
        Add(Encoding.UTF8.GetBytes("blob1|"));
        foreach (var record in records) { var bytes = Encoding.UTF8.GetBytes(record); Add(Encoding.ASCII.GetBytes(bytes.Length.ToString(CultureInfo.InvariantCulture) + ":")); Add(bytes); }
        return "blob1-" + hash.ToString("x16", CultureInfo.InvariantCulture);
    }
    private static JournalVersion ReadOne(IAtomicJournalStore store, string key)
    {
        var result = store.Read(new[] { key });
        if (result.Count != 1 || result[0] is null || result[0].Entries is null || result[0].Entries.Any(value => value is null)) throw new InvalidDataException("Invalid immutable object snapshot.");
        if (result[0].Generation == 0 && result[0].Entries.Count != 0) throw new InvalidDataException("Invalid absent immutable object.");
        return new(result[0].Generation, result[0].Entries.ToArray());
    }
    private string[] ReadObject(string partition, JournalChunks.Reference reference)
    {
        if (_cache?.Get(partition, reference.Key, reference.Generation) is { } cached)
        {
            JournalChunks.ValidateChunk(cached, reference.Records); return cached;
        }
        var data = ReadOne(_partitions[partition], reference.Key);
        if (data.Generation != reference.Generation || Address(data.Entries) != reference.Key) throw new InvalidDataException("Missing or modified immutable object.");
        JournalChunks.ValidateChunk(data.Entries, reference.Records);
        _cache?.Remember(partition, reference.Key, reference.Generation, data.Entries);
        return data.Entries.ToArray();
    }
    public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys)
    {
        ClusterEngine<uint>.ValidateKeys(keys);
        var pointers = _catalog.Read(keys);
        if (pointers.Count != keys.Count) throw new InvalidDataException("Incomplete catalog snapshot.");
        // Complete all selected metadata validation before any data-store read.
        var selected = pointers.Select(pointer =>
        {
            if (pointer is null || pointer.Entries is null) throw new InvalidDataException("Invalid catalog snapshot.");
            if (pointer.Generation == 0 && pointer.Entries.Count == 0) return (Generation: 0UL, Manifest: (JournalChunks.Manifest?)null);
            if (pointer.Generation == 0) throw new InvalidDataException("Invalid catalog generation.");
            var manifest = JournalChunks.Decode(pointer.Entries.ToArray());
            if (!_partitions.ContainsKey(manifest.Partition)) throw new InvalidDataException("Invalid catalog pointer partition.");
            return (pointer.Generation, Manifest: (JournalChunks.Manifest?)manifest);
        }).ToArray();
        var result = new List<JournalVersion>();
        foreach (var (generation, manifest) in selected)
        {
            var records = new List<string>();
            if (manifest is not null) foreach (var reference in manifest.References) records.AddRange(ReadObject(manifest.Partition, reference));
            result.Add(new(generation, records.ToArray()));
        }
        return result;
    }
    public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
    {
        ClusterEngine<uint>.ValidateKeys(keys);
        if (expected.Count != keys.Count || updates.Count != keys.Count) throw new ArgumentException("Expected generations and updates must match shards.", nameof(updates));
        var versions = expected.Select(item => new JournalVersion(item.Generation, Array.Empty<string>())).ToArray();
        // Validate routes and copy all updates before any external staging.
        var staged = keys.Select((key, index) =>
        {
            if (updates[index] is not { } entries) return (Data: (string[]?)null, Partition: "", Objects: Array.Empty<IReadOnlyList<string>>());
            var records = entries.ToArray();
            if (records.Any(value => value is null)) throw new ArgumentException("Journal must contain strings.", nameof(updates));
            var partition = _route(key);
            if (!_partitions.ContainsKey(partition)) throw new ArgumentException("Route must select a configured partition.", nameof(keys));
            return (Data: (string[]?)records, Partition: partition, Objects: _chunks is null ? new IReadOnlyList<string>[] { records } : JournalChunks.Split(records, _chunks));
        }).ToArray();
        var pointers = new List<IReadOnlyList<string>?>();
        foreach (var (data, partition, objects) in staged)
        {
            if (data is null) { pointers.Add(null); continue; }
            var manifest = _chunks is null ? new List<string> { "blob1", partition } : new List<string> { "chunks1", partition, data.Length.ToString(CultureInfo.InvariantCulture), objects.Length.ToString(CultureInfo.InvariantCulture) };
            foreach (var chunk in objects)
            {
                var key = Address(chunk); var store = _partitions[partition]; var item = ReadOne(store, key);
                if (item.Generation == 0)
                {
                    // An uncertain write leaves an orphan; mutations are never retried.
                    store.CompareExchange(new[] { key }, new[] { item }, new IReadOnlyList<string>?[] { chunk });
                    item = ReadOne(store, key);
                }
                if (item.Generation == 0 || !item.Entries.SequenceEqual(chunk)) throw new InvalidDataException("Immutable object collision or inconsistent data store.");
                manifest.Add(key); manifest.Add(item.Generation.ToString(CultureInfo.InvariantCulture));
                if (_chunks is not null) manifest.Add(chunk.Count.ToString(CultureInfo.InvariantCulture));
            }
            pointers.Add(manifest.ToArray());
        }
        return _catalog.CompareExchange(keys, versions, pointers);
    }
}
