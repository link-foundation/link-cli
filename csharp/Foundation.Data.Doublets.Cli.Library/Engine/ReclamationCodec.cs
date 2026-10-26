using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;

namespace Foundation.Data.Doublets.Cli.Engine;

internal static class ReclamationCodec
{
    static ReclamationCodec() { }
    private static readonly UTF8Encoding Encoding = new(false, true);
    internal const string EpochKey = "__link_gc_epoch";
    internal sealed record Pointer(string Partition, string Key, ulong Generation, ulong Epoch);
    internal static string[] Keys(IReadOnlyList<string> keys)
    {
        ClusterEngine<uint>.ValidateKeys(keys);
        if (keys.Count > 31 || keys.Contains(EpochKey)) throw new ArgumentException("Select 1..31 nonreserved shards.", nameof(keys));
        return keys.ToArray();
    }
    internal static string[] Records(IReadOnlyList<string> records)
    {
        ArgumentNullException.ThrowIfNull(records);
        var owned = records.ToArray();
        foreach (var record in owned)
        {
            if (record is null) throw new ArgumentException("Journal must contain strings.", nameof(records));
            Encoding.GetByteCount(record);
        }
        return owned;
    }
    internal static JournalVersion Version(JournalVersion value, bool absent = true)
    {
        if (value is null || (value.Generation == 0 && (!absent || value.Entries.Count != 0))) throw new InvalidDataException("Invalid reclamation generation.");
        return new(value.Generation, Records(value.Entries));
    }
    internal static JournalVersion[] Read(IAtomicJournalStore provider, IReadOnlyList<string> keys)
    {
        var values = provider.Read(keys);
        if (values is null || values.Count != keys.Count) throw new InvalidDataException("Incomplete reclamation snapshot.");
        return values.Select(value => Version(value)).ToArray();
    }
    private static bool Canonical(string text, out ulong value) => ulong.TryParse(text, NumberStyles.None, CultureInfo.InvariantCulture, out value) && value.ToString(CultureInfo.InvariantCulture) == text;
    internal static ulong Epoch(JournalVersion value)
    {
        value = Version(value, false);
        if (value.Entries.Count != 2 || value.Entries[0] != "gc1" || !Canonical(value.Entries[1], out var epoch)) throw new InvalidDataException("Invalid reclamation epoch.");
        return epoch;
    }
    internal static ulong KeyEpoch(string key)
    {
        if (!Regex.IsMatch(key, "\\Ablob2-[0-9a-f]{16}-[0-9a-f]{16}\\z", RegexOptions.CultureInvariant)) throw new InvalidDataException("Invalid reclamation object key.");
        return ulong.Parse(key.AsSpan(6, 16), NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture);
    }
    internal static string Address(ulong epoch, IReadOnlyList<string> records)
    {
        var hash = 0xcbf29ce484222325UL;
        void Add(byte[] bytes) { foreach (var value in bytes) hash = unchecked((hash ^ value) * 0x100000001b3UL); }
        Add(Encoding.GetBytes("blob2|"));
        foreach (var record in records) { var bytes = Encoding.GetBytes(record); Add(Encoding.GetBytes(bytes.Length.ToString(CultureInfo.InvariantCulture) + ":")); Add(bytes); }
        return "blob2-" + epoch.ToString("x16", CultureInfo.InvariantCulture) + "-" + hash.ToString("x16", CultureInfo.InvariantCulture);
    }
    internal static Pointer? ParsePointer(JournalVersion value, ulong epoch, IReadOnlyDictionary<string, IJournalMaintenanceStore> partitions)
    {
        value = Version(value);
        if (value.Generation == 0) return null;
        var parts = value.Entries;
        if (parts.Count != 4 || parts[0] != "blob2" || !partitions.ContainsKey(parts[1]) || !Canonical(parts[3], out var generation) || generation == 0) throw new InvalidDataException("Invalid reclamation pointer.");
        var objectEpoch = KeyEpoch(parts[2]);
        if (objectEpoch > epoch) throw new InvalidDataException("Pointer epoch exceeds catalog epoch.");
        return new(parts[1], parts[2], generation, objectEpoch);
    }
    internal static JournalNamespaceEntry[] Scan(IJournalMaintenanceStore provider, ReclamationOptions options, bool keysOnly)
    {
        var values = provider.ScanNamespace(new(options.MaxKeys, options.MaxBytes, options.PageKeys, keysOnly));
        if (values is null || values.Count > options.MaxKeys) throw new InvalidDataException("Invalid complete namespace snapshot.");
        var result = new List<JournalNamespaceEntry>(); string previous = ""; long bytes = 0;
        foreach (var item in values)
        {
            ClusterEngine<uint>.ValidateKeys(new[] { item.Key });
            if (string.CompareOrdinal(item.Key, previous) <= 0) throw new InvalidDataException("Unordered namespace snapshot.");
            var version = Version(item.Version, false);
            if (keysOnly && version.Entries.Count != 0) throw new InvalidDataException("Unexpected keys-only records.");
            bytes = checked(bytes + Encoding.GetByteCount(item.Key) + 8);
            foreach (var record in version.Entries) bytes = checked(bytes + Encoding.GetByteCount(record) + 1);
            if (bytes > options.MaxBytes) throw new InvalidDataException("Complete namespace byte limit.");
            result.Add(new(item.Key, version)); previous = item.Key;
        }
        return result.ToArray();
    }
    internal static (ulong Epoch, Dictionary<(string Partition, string Key), ulong> Protected) Catalog(IReadOnlyList<JournalNamespaceEntry> entries, IReadOnlyDictionary<string, IJournalMaintenanceStore> partitions)
    {
        var root = entries.SingleOrDefault(item => item.Key == EpochKey) ?? throw new InvalidDataException("Missing reclamation epoch.");
        var epoch = Epoch(root.Version); var protectedObjects = new Dictionary<(string, string), ulong>();
        foreach (var item in entries)
        {
            if (item.Key == EpochKey) continue;
            var pointer = ParsePointer(item.Version, epoch, partitions) ?? throw new InvalidDataException("Absent catalog entry.");
            var identity = (pointer.Partition, pointer.Key);
            if (protectedObjects.TryGetValue(identity, out var prior) && prior != pointer.Generation) throw new InvalidDataException("Contradictory protected generations.");
            protectedObjects[identity] = pointer.Generation;
        }
        return (epoch, protectedObjects);
    }
}
