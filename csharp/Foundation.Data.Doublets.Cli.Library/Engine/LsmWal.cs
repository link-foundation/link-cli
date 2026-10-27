using System.Globalization;
using System.Text;
using System.Text.Json;
namespace Foundation.Data.Doublets.Cli.Engine;

public sealed record LsmOptions(int MaxKeys = 4096, int MaxBytes = 8_388_608,
    int BufferKeys = 64, int BufferBytes = 1_048_576, int MaxRuns = 4,
    int MaxWalRecords = 1024, int MaxWalBytes = 16_777_216)
{
    public void Validate()
    {
        foreach (var (value, maximum) in new[] { (MaxKeys, 65536), (MaxBytes, 8388608),
            (BufferKeys, 65536), (BufferBytes, 8388608), (MaxRuns, 64),
            (MaxWalRecords, 65536), (MaxWalBytes, 67108864) })
            if (value < 1 || value > maximum) throw new ArgumentOutOfRangeException(nameof(value));
        if (BufferKeys > MaxKeys || BufferBytes > MaxBytes) throw new ArgumentException("LSM buffer exceeds total admission.");
    }
}
public sealed record LsmTableDescriptor(ulong Id, string Identity, ulong Digest, int Count, int StoredBytes);
/// <summary>Owned, transient semantic replay result. A physical store must also
/// validate every selected table and combined capacity before exposing data.</summary>
public sealed record LsmWalState(ulong Generation, ulong CoveredGeneration, ulong LastReservedTableId,
    IReadOnlyList<LsmTableDescriptor> Manifest, IReadOnlyList<SortedJournalEntry> Buffer);
/// <summary>Optional native-lsm1 replay and encoding. Performs no storage I/O.</summary>
public static class LsmWal
{
    private static readonly UTF8Encoding Utf8 = new(false, true);
    private static InvalidDataException Invalid(string message) => new("Invalid LSM WAL: " + message);
    private static string Text(ulong value) => value.ToString(CultureInfo.InvariantCulture);
    private static ulong Counter(JsonElement value, bool positive = false)
    {
        if (value.ValueKind != JsonValueKind.String) throw Invalid("counter type");
        string text = value.GetString()!;
        if (text.Length is < 1 or > 20 || !ulong.TryParse(text, NumberStyles.None, CultureInfo.InvariantCulture, out ulong result)
            || Text(result) != text || (positive && result == 0)) throw Invalid("canonical u64");
        return result;
    }
    private static void Shape(JsonElement value, int count)
    { if (value.ValueKind != JsonValueKind.Array || value.GetArrayLength() != count) throw Invalid("record shape"); }
    private static SortedJournalEntry[] Rows(JsonElement value, ulong lower, ulong upper, int maxKeys, int maxBytes)
    {
        if (value.ValueKind != JsonValueKind.Array || value.GetArrayLength() > maxKeys) throw Invalid("row count");
        var rows = new List<SortedJournalEntry>(); string previous = ""; long bytes = 0;
        foreach (var item in value.EnumerateArray())
        {
            Shape(item, 3);
            if (item[0].ValueKind != JsonValueKind.String) throw Invalid("row key type");
            string key = item[0].GetString()!;
            if (key.Length is < 1 or > 128 || string.CompareOrdinal(key, previous) <= 0
                || key.Any(c => !char.IsAsciiLetterOrDigit(c) && c is not '_' and not '.' and not '-')) throw Invalid("row key ordering");
            previous = key; ulong generation = Counter(item[1], true);
            if (generation <= lower || generation > upper) throw Invalid("row generation");
            bytes += key.Length + 8; string[]? entries = null;
            if (item[2].ValueKind != JsonValueKind.Null)
            {
                if (item[2].ValueKind != JsonValueKind.Array) throw Invalid("row payload");
                var records = new List<string>();
                foreach (var record in item[2].EnumerateArray())
                {
                    if (record.ValueKind != JsonValueKind.String) throw Invalid("record string");
                    string text = record.GetString()!; bytes += Utf8.GetByteCount(text) + 1L;
                    if (bytes > maxBytes) throw Invalid("buffer bytes"); records.Add(text);
                }
                entries = records.ToArray();
            }
            if (bytes > maxBytes) throw Invalid("buffer bytes"); rows.Add(new(key, generation, entries));
        }
        return rows.ToArray();
    }
    private static long LogicalBytes(IEnumerable<SortedJournalEntry> rows)
    {
        long bytes = 0;
        foreach (var row in rows)
        {
            bytes += row.Key.Length + 8;
            if (row.Entries is not null) foreach (string record in row.Entries) bytes += Utf8.GetByteCount(record) + 1L;
        }
        return bytes;
    }
    public static LsmWalState Recover(IReadOnlyList<string> records, LsmOptions? options = null)
    {
        ArgumentNullException.ThrowIfNull(records); options ??= new(); options.Validate();
        if (records.Count < 1 || records.Count > options.MaxWalRecords) throw Invalid("record count");
        long bytes = 0;
        foreach (string record in records)
        {
            ArgumentNullException.ThrowIfNull(record); bytes += Utf8.GetByteCount(record);
            if (record.Contains('\r') || record.Contains('\n') || bytes > options.MaxWalBytes) throw Invalid("physical record or WAL bytes");
        }
        using var checkpoint = JsonDocument.Parse(records[0]); var root = checkpoint.RootElement; Shape(root, 6);
        if (root[0].ValueKind != JsonValueKind.String || root[0].GetString() != "native-lsm1") throw Invalid("checkpoint format");
        ulong generation = Counter(root[1]), covered = Counter(root[2]), reserved = Counter(root[3]);
        if (covered > generation || root[4].ValueKind != JsonValueKind.Array || root[4].GetArrayLength() > options.MaxRuns) throw Invalid("manifest bounds");
        var manifest = new List<LsmTableDescriptor>(); ulong previous = 0;
        foreach (var item in root[4].EnumerateArray())
        {
            Shape(item, 5); ulong id = Counter(item[0], true);
            if (id <= previous || id > reserved) throw Invalid("table ID ordering or reservation"); previous = id;
            if (item[1].ValueKind != JsonValueKind.String) throw Invalid("table identity type");
            string identity = item[1].GetString()!;
            if (identity.Length != 32 || identity.Any(c => !char.IsAsciiDigit(c) && c is not (>= 'a' and <= 'f'))) throw Invalid("native table identity");
            ulong digest = Counter(item[2]), count = Counter(item[3]), stored = Counter(item[4]);
            if (count > (ulong)options.MaxKeys || stored is < 15 or > 67108864) throw Invalid("table admission");
            manifest.Add(new(id, identity, digest, (int)count, (int)stored));
        }
        if ((covered != 0) != (manifest.Count != 0)) throw Invalid("covered table presence");
        var buffer = Rows(root[5], covered, generation, options.BufferKeys, options.BufferBytes);
        if (buffer.Select(row => row.Generation).DefaultIfEmpty(covered).Max() != generation) throw Invalid("missing latest generation");
        for (int i = 1; i < records.Count; i++)
        {
            using var parsed = JsonDocument.Parse(records[i]); var record = parsed.RootElement; Shape(record, 3);
            if (record[0].ValueKind != JsonValueKind.String || record[0].GetString() != "put") throw Invalid("group format");
            ulong next = Counter(record[1], true);
            if (generation == ulong.MaxValue || next != generation + 1) throw Invalid("group sequence");
            var changes = Rows(record[2], generation, next, 32, options.BufferBytes);
            if (changes.Length == 0) throw Invalid("empty group");
            var merged = new SortedDictionary<string, SortedJournalEntry>(StringComparer.Ordinal);
            foreach (var row in buffer) merged.Add(row.Key, row);
            foreach (var row in changes) merged[row.Key] = row;
            buffer = merged.Values.ToArray();
            if (buffer.Length > options.BufferKeys || LogicalBytes(buffer) > options.BufferBytes) throw Invalid("replay buffer admission");
            generation = next;
        }
        return new(generation, covered, reserved, manifest.ToArray(), buffer);
    }
    public static string Encode(LsmWalState state, LsmOptions? options = null)
    {
        ArgumentNullException.ThrowIfNull(state); ArgumentNullException.ThrowIfNull(state.Manifest); ArgumentNullException.ThrowIfNull(state.Buffer);
        var manifest = state.Manifest.Select(item => new object[] { Text(item.Id), item.Identity, Text(item.Digest), item.Count.ToString(CultureInfo.InvariantCulture), item.StoredBytes.ToString(CultureInfo.InvariantCulture) }).ToArray();
        var buffer = state.Buffer.Select(row => new object?[] { row.Key, Text(row.Generation), row.Entries }).ToArray();
        foreach (var row in state.Buffer) if (row.Entries is not null) foreach (string record in row.Entries) { ArgumentNullException.ThrowIfNull(record); _ = Utf8.GetByteCount(record); }
        string encoded = JsonSerializer.Serialize(new object[] { "native-lsm1", Text(state.Generation), Text(state.CoveredGeneration), Text(state.LastReservedTableId), manifest, buffer });
        _ = Recover(new[] { encoded }, options); return encoded;
    }
}
