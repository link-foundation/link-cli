using System.Globalization;
using System.Text;
using System.Text.Json;
using static Foundation.Data.Doublets.Cli.Engine.PageFormat;
namespace Foundation.Data.Doublets.Cli.Engine;

public sealed record SortedJournalEntry(string Key, ulong Generation, IReadOnlyList<string>? Entries);
/// <summary>Total encoded UTF-8 native record bytes include the format marker.</summary>
public sealed record SortedTableOptions(int MaxKeys = 4096, int MaxBytes = 8_388_608);
/// <summary>Caller-owned exclusive immutable doublets pages. Full bounded startup
/// validation retains keys/physical addresses and hashes, without a payload map.
/// Read failures never mutate storage. The caller closes its selected device.</summary>
public sealed class SortedJournalTable
{
    private const string Marker = "[\"native-sst1\"]";
    private static readonly UTF8Encoding Utf8 = new(false, true);
    private sealed record Index(string Key, ulong Generation, bool Tombstone, ulong Root, ulong Previous);
    private readonly IPageDevice _device;
    private readonly Metadata _state;
    private readonly List<Index> _index;
    private readonly ulong[] _hashes;
    public int Count => _index.Count;
    public int StoredBytes { get; }
    public byte[] Identity => _state.Identity.ToArray();
    public ulong Digest => _state.LastHash;
    public bool SupportsDurableFlush => _device.SupportsDurableFlush;
    private SortedJournalTable(IPageDevice device, Metadata state, List<Index> index, ulong[] hashes, int bytes)
    { _device = device; _state = state; _index = index; _hashes = hashes; StoredBytes = bytes; }
    private static ulong Pages(SortedTableOptions options)
    {
        if (options.MaxKeys is < 1 or > 65536 || options.MaxBytes is < 1 or > 67108864) throw new ArgumentOutOfRangeException(nameof(options));
        return ((ulong)(options.MaxBytes + 6) / 7 + 2 * ((ulong)options.MaxKeys + 1) + Cells - 1) / Cells;
    }
    private static void Key(string key)
    {
        if (string.IsNullOrEmpty(key) || key.Length > 128 || key.Any(c => !char.IsAsciiLetterOrDigit(c) && c is not '.' and not '_' and not '-')) throw new ArgumentException("Invalid sorted table key.");
    }
    public static SortedJournalTable Create(IPageDevice device, IReadOnlyList<SortedJournalEntry> entries, SortedTableOptions? options = null)
    {
        ArgumentNullException.ThrowIfNull(device); ArgumentNullException.ThrowIfNull(entries); options ??= new(); ulong pages = Pages(options);
        if (entries.Count > options.MaxKeys) throw Invalid("sorted table key limit");
        var records = new List<string> { Marker }; long bytes = Utf8.GetByteCount(Marker); string previous = "";
        foreach (var entry in entries)
        {
            ArgumentNullException.ThrowIfNull(entry); Key(entry.Key);
            if (string.CompareOrdinal(entry.Key, previous) <= 0 || entry.Generation == 0) throw new ArgumentException("Invalid sorted table ordering or generation.");
            if (entry.Entries is not null) foreach (var record in entry.Entries) { ArgumentNullException.ThrowIfNull(record); _ = Utf8.GetByteCount(record); }
            string encoded = JsonSerializer.Serialize(new object?[] { entry.Key, entry.Generation.ToString(CultureInfo.InvariantCulture), entry.Entries });
            bytes += Utf8.GetByteCount(encoded); if (bytes > options.MaxBytes) throw Invalid("sorted table byte limit");
            records.Add(encoded); previous = entry.Key;
        }
        if (bytes > options.MaxBytes) throw Invalid("sorted table byte limit");
        var raw = new[] { device.ReadMetadata(0), device.ReadMetadata(1) };
        if (raw.Any(value => value is { Length: > 0 }) || device.DataLength != 0) throw Invalid("sorted table creation requires an empty device");
        var log = new PagedCommitLog(device, pages); foreach (var record in records) log.Append(record); log.Flush(); return Open(device, options);
    }
    public static SortedJournalTable Open(IPageDevice device, SortedTableOptions? options = null)
    {
        ArgumentNullException.ThrowIfNull(device); options ??= new(); ulong maxPages = Pages(options); uint flags = device.MetadataFlags;
        if (flags > 1) throw Invalid("unsupported sorted table device mode");
        var raw = new[] { device.ReadMetadata(0), device.ReadMetadata(1) }; var first = Metadata.Decode(raw[0], flags); var second = Metadata.Decode(raw[1], flags);
        if (first is not null && second is not null && (!first.Identity.AsSpan().SequenceEqual(second.Identity) || (first.Generation == second.Generation && !raw[0]!.AsSpan().SequenceEqual(raw[1])))) throw Invalid("conflicting sorted table decisions");
        var state = first is null || (second is not null && second.Generation > first.Generation) ? second : first;
        if (state is null || state.Generation != 1 || state.Records > (ulong)options.MaxKeys + 1 || state.Pages > maxPages || device.DataLength != checked((long)state.Pages * PageSize)) throw Invalid("sorted table seal, length or admission bounds");
        var records = PagedCommitLog.ReadCommitted(device, state, retain: true);
        if (records.Count == 0 || records[0] != Marker) throw Invalid("unsupported sorted table marker");
        var index = new List<Index>(); long bytes = 0; ulong root = 0, previousRoot = 0; string previous = "";
        for (int i = 0; i < records.Count; i++)
        {
            int length = Utf8.GetByteCount(records[i]); bytes += length; if (bytes > options.MaxBytes) throw Invalid("sorted table byte limit"); root += ((ulong)length + 6) / 7 + 1;
            if (i != 0)
            {
                var entry = Decode(records[i]); if (string.CompareOrdinal(entry.Key, previous) <= 0) throw Invalid("sorted table key ordering");
                index.Add(new(entry.Key, entry.Generation, entry.Entries is null, root, previousRoot)); previous = entry.Key;
            }
            previousRoot = root;
        }
        if (root != state.Root || root != state.Rows) throw Invalid("sorted table packed addresses");
        var hashes = new ulong[checked((int)state.Pages)]; ulong expected = state.LastHash;
        for (ulong end = state.Pages; end > 0; end--)
        {
            ulong page = end - 1; var data = device.ReadCheckedPage(page, expected);
            if (data.Length != PageSize || PageHash(data) != expected || U64(data, 56) != expected) throw Invalid("sorted table page changed during open");
            hashes[(int)page] = expected; expected = U64(data, 48);
        }
        device.CompleteRecovery(); return new(device, state, index, hashes, checked((int)bytes));
    }
    public SortedJournalEntry? Get(string key)
    {
        Key(key); int low = 0, high = _index.Count - 1;
        while (low <= high) { int mid = low + (high - low) / 2; int order = string.CompareOrdinal(_index[mid].Key, key); if (order == 0) return At(mid); if (order < 0) low = mid + 1; else high = mid - 1; }
        return null;
    }
    public IReadOnlyList<SortedJournalEntry> Entries() => Enumerable.Range(0, Count).Select(At).ToArray();
    private SortedJournalEntry At(int position)
    {
        var entry = _index[position]; ulong page = (entry.Root - 1) / Cells; var cursor = new PagedCommitLog.Cursor(_device, _state, _hashes[(int)page]); cursor.StartAt(entry.Root);
        var (source, previous) = cursor.Take(entry.Root); ulong text = Portable(source); using var bytes = new MemoryStream();
        while (text != 0)
        {
            var (external, next) = cursor.Take(text); int length = (int)((external >> 56) & 7); ulong payload = external & Payload;
            if ((external & External) == 0 || (external & 0x7800000000000000) != 0 || length == 0 || payload >> (length * 8) != 0 || (next != 0 && length != 7)) throw Invalid("sorted table string doublet");
            for (int i = 0; i < length; i++) bytes.WriteByte((byte)(payload >> (i * 8))); text = next;
        }
        if (previous != entry.Previous || cursor.Current != previous) throw Invalid("sorted table record predecessor");
        var value = Decode(Utf8.GetString(bytes.GetBuffer(), 0, checked((int)bytes.Length)));
        if (value.Key != entry.Key || value.Generation != entry.Generation || (value.Entries is null) != entry.Tombstone) throw Invalid("sorted table index identity"); return value;
    }
    private static SortedJournalEntry Decode(string record)
    {
        using var doc = JsonDocument.Parse(record); var row = doc.RootElement;
        if (row.ValueKind != JsonValueKind.Array || row.GetArrayLength() != 3 || row[0].ValueKind != JsonValueKind.String || row[1].ValueKind != JsonValueKind.String) throw Invalid("sorted table record shape");
        string key = row[0].GetString()!; Key(key); string text = row[1].GetString()!;
        if (!ulong.TryParse(text, NumberStyles.None, CultureInfo.InvariantCulture, out ulong generation) || generation == 0 || generation.ToString(CultureInfo.InvariantCulture) != text) throw Invalid("sorted table canonical generation");
        string[]? entries = null;
        if (row[2].ValueKind != JsonValueKind.Null)
        {
            if (row[2].ValueKind != JsonValueKind.Array) throw Invalid("sorted table records");
            entries = row[2].EnumerateArray().Select(value => value.ValueKind == JsonValueKind.String ? value.GetString()! : throw Invalid("sorted table record type")).ToArray();
            foreach (string value in entries) _ = Utf8.GetByteCount(value);
        }
        return new(key, generation, entries);
    }
}
