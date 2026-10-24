using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Immutable journal objects in independent stores, with small pointers
/// published atomically in a catalog. Stores are caller-owned. Object namespaces
/// must remain append-only; deleting or rewriting objects invalidates snapshots.</summary>
public sealed class PartitionedJournalStore : IAtomicJournalStore
{
    private readonly IAtomicJournalStore _catalog;
    private readonly Dictionary<string, IAtomicJournalStore> _partitions;
    private readonly Func<string, string> _route;
    public PartitionedJournalStore(IAtomicJournalStore catalog, IReadOnlyDictionary<string, IAtomicJournalStore> partitions, Func<string, string> route)
    {
        ArgumentNullException.ThrowIfNull(catalog); ArgumentNullException.ThrowIfNull(partitions); ArgumentNullException.ThrowIfNull(route);
        if (partitions.Count == 0) throw new ArgumentException("Provide named atomic data stores.", nameof(partitions));
        _partitions = new(StringComparer.Ordinal);
        foreach (var (name, store) in partitions) { ClusterEngine<uint>.ValidateKeys(new[] { name }); ArgumentNullException.ThrowIfNull(store); _partitions.Add(name, store); }
        _catalog = catalog; _route = route;
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
        if (result.Count != 1) throw new InvalidDataException("Invalid immutable object snapshot.");
        return new(result[0].Generation, result[0].Entries.ToArray());
    }
    private static bool Generation(string value, out ulong generation) => ulong.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out generation) && generation > 0 && generation.ToString(CultureInfo.InvariantCulture) == value;
    public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys)
    {
        ClusterEngine<uint>.ValidateKeys(keys);
        var pointers = _catalog.Read(keys);
        if (pointers.Count != keys.Count) throw new InvalidDataException("Incomplete catalog snapshot.");
        pointers = pointers.Select(item => new JournalVersion(item.Generation, item.Entries.ToArray())).ToArray();
        var result = new List<JournalVersion>();
        foreach (var pointer in pointers)
        {
            if (pointer.Generation == 0 && pointer.Entries.Count == 0) { result.Add(pointer); continue; }
            var parts = pointer.Entries;
            if (pointer.Generation == 0 || parts.Count != 4 || parts[0] != "blob1" || !_partitions.TryGetValue(parts[1], out var store) || !Regex.IsMatch(parts[2], "\\Ablob1-[0-9a-f]{16}\\z", RegexOptions.CultureInvariant) || !Generation(parts[3], out var generation))
                throw new InvalidDataException("Invalid catalog pointer.");
            var data = ReadOne(store, parts[2]);
            if (data.Generation != generation || Address(data.Entries) != parts[2]) throw new InvalidDataException("Missing or modified immutable object.");
            result.Add(new(pointer.Generation, data.Entries));
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
            if (updates[index] is not { } entries) return (Data: (string[]?)null, Partition: "");
            var partition = _route(key);
            if (!_partitions.ContainsKey(partition)) throw new ArgumentException("Route must select a configured partition.", nameof(keys));
            return (Data: entries.ToArray(), Partition: partition);
        }).ToArray();
        var pointers = new List<IReadOnlyList<string>?>();
        foreach (var (data, partition) in staged)
        {
            if (data is null) { pointers.Add(null); continue; }
            var key = Address(data); var store = _partitions[partition]; var item = ReadOne(store, key);
            if (item.Generation == 0)
            {
                // An uncertain write leaves an orphan; mutations are never retried.
                store.CompareExchange(new[] { key }, new[] { item }, new IReadOnlyList<string>?[] { data });
                item = ReadOne(store, key);
            }
            if (item.Generation == 0 || !item.Entries.SequenceEqual(data)) throw new InvalidDataException("Immutable object collision or inconsistent data store.");
            pointers.Add(new[] { "blob1", partition, key, item.Generation.ToString(CultureInfo.InvariantCulture) });
        }
        return _catalog.CompareExchange(keys, versions, pointers);
    }
}
