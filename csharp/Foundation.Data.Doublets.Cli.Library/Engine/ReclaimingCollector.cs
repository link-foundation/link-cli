using System.Globalization;
namespace Foundation.Data.Doublets.Cli.Engine;

public sealed partial class ReclaimingPartitionedJournalStore
{
    /// <summary>One epoch-fenced collection attempt. Null means a known competing
    /// fence. Errors may follow acknowledged deletions; mutations are not replayed.</summary>
    public ReclamationResult? CollectGarbage()
    {
        var before = ReclamationCodec.Read(_catalog, new[] { ReclamationCodec.EpochKey })[0];
        var fence = checked(ReclamationCodec.Epoch(before) + 1);
        if (!_catalog.CompareExchange(new[] { ReclamationCodec.EpochKey }, new[] { before }, new IReadOnlyList<string>?[] { new[] { "gc1", fence.ToString(CultureInfo.InvariantCulture) } })) return null;
        var (epoch, protectedObjects) = ReclamationCodec.Catalog(ReclamationCodec.Scan(_catalog, _options, false), _partitions);
        if (epoch < fence) throw new InvalidDataException("Catalog snapshot predates reclamation fence.");
        // Providers may alias a physical namespace. Retain live keys across names.
        var protectedKeys = protectedObjects.Keys.Select(identity => identity.Key).ToHashSet(StringComparer.Ordinal);
        var snapshots = new List<(string Partition, IJournalMaintenanceStore Provider, JournalNamespaceEntry[] Items)>();
        foreach (var (partition, provider) in _partitions)
        {
            var items = ReclamationCodec.Scan(provider, _options, true); var observed = new Dictionary<string, ulong>(StringComparer.Ordinal);
            foreach (var item in items) { ReclamationCodec.KeyEpoch(item.Key); observed.Add(item.Key, item.Version.Generation); }
            foreach (var (identity, generation) in protectedObjects)
                if (identity.Partition == partition && (!observed.TryGetValue(identity.Key, out var actual) || actual != generation)) throw new InvalidDataException("Missing or changed protected object.");
            snapshots.Add((partition, provider, items));
        }
        long examined = 0, deleted = 0;
        // Every namespace validates before the first destructive action.
        foreach (var (partition, provider, items) in snapshots)
        {
            foreach (var item in items)
            {
                examined = checked(examined + 1);
                if (ReclamationCodec.KeyEpoch(item.Key) >= fence || protectedKeys.Contains(item.Key)) continue;
                if (provider.CompareDelete(item.Key, item.Version.Generation)) deleted++;
            }
        }
        return new(fence, examined, deleted);
    }
}
