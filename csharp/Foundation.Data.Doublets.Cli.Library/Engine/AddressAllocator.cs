using System.Globalization;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace Foundation.Data.Doublets.Cli.Engine;

public sealed record AddressAllocatorOptions(int MaxEntries = 4096, int MaxBytes = 1_048_576, bool RequireDurableFlush = false);
public sealed record AddressReservation(string Author, string Token, ulong Count, ulong First, ulong Last);
public sealed record AddressAllocationSnapshot(ulong Generation, ulong? Next, IReadOnlyList<AddressReservation> Reservations);

/// <summary>Selected permanent fresh ranges over one linearizable atomic key.
/// Every fresh writer must use this key and immutable domain, initialized above
/// every previously issued identity. Grants and retry receipts are never recycled.
/// O(retained journal) per new grant; the provider and author fencing are caller-owned.</summary>
public sealed class AddressAllocator
{
    private readonly IAtomicJournalStore _store;
    private readonly string _key, _header;
    private readonly ulong _first, _last;
    private readonly AddressAllocatorOptions _options;
    public AddressAllocator(IAtomicJournalStore store, string key, ulong first, ulong last, AddressAllocatorOptions? options = null)
    {
        options ??= new();
        if (store is null || key is null || !Regex.IsMatch(key, "\\A[A-Za-z0-9_.-]{1,128}\\z", RegexOptions.CultureInvariant) ||
            first == 0 || first > last || options.MaxEntries is < 2 or > 4096 || options.MaxBytes is < 1 or > 1_048_576 ||
            (options.RequireDurableFlush && !store.SupportsDurableFlush)) throw Invalid();
        _store = store; _key = key; _first = first; _last = last; _options = options;
        _header = JsonSerializer.Serialize(new[] { "address-reservations1", Text(first), Text(last) });
    }
    private static InvalidDataException Invalid() => new("Invalid address reservation state or request.");
    private static string Text(ulong value) => value.ToString(CultureInfo.InvariantCulture);
    private static void Identity(string value)
    {
        if (value is null || value.Length != 32 || value.All(c => c == '0') ||
            value.Any(c => !(c is >= '0' and <= '9' or >= 'a' and <= 'f'))) throw Invalid();
    }
    private static ulong Number(string value)
    {
        if (value.Length is < 1 or > 20 || (value.Length > 1 && value[0] == '0') || value.Any(c => c is < '0' or > '9') ||
            !ulong.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out ulong number)) throw Invalid();
        return number;
    }
    private void Admit(IReadOnlyList<string> entries)
    {
        if (entries is null || entries.Count > _options.MaxEntries) throw Invalid();
        int bytes = 0;
        foreach (string entry in entries)
        {
            // Accepted fields are ASCII; bound before parsing any JSON.
            if (entry is null || entry.Length > 256 || (bytes += entry.Length) > _options.MaxBytes) throw Invalid();
        }
    }
    private (JournalVersion Expected, UInt128 Next, List<AddressReservation> Reservations) Load()
    {
        var values = _store.Read([_key]);
        if (values is null || values.Count != 1 || values[0] is null) throw Invalid();
        Admit(values[0].Entries);
        var expected = values[0] with { Entries = values[0].Entries.ToArray() };
        UInt128 next = _first; var reservations = new List<AddressReservation>(); var tokens = new HashSet<string>(StringComparer.Ordinal);
        if (expected.Generation == 0)
        {
            if (expected.Entries.Count != 0) throw Invalid();
        }
        else
        {
            if (expected.Entries.Count == 0 || expected.Entries[0] != _header) throw Invalid();
            foreach (string text in expected.Entries.Skip(1))
            {
                using var document = JsonDocument.Parse(text);
                var root = document.RootElement;
                if (root.ValueKind != JsonValueKind.Array || root.GetArrayLength() != 6 ||
                    root.EnumerateArray().Any(v => v.ValueKind != JsonValueKind.String)) throw Invalid();
                var entry = root.EnumerateArray().Select(v => v.GetString()!).ToArray();
                if (entry[0] != "reserve-addresses1" || JsonSerializer.Serialize(entry) != text) throw Invalid();
                Identity(entry[1]); Identity(entry[2]);
                ulong count = Number(entry[3]), first = Number(entry[4]), last = Number(entry[5]);
                if (count == 0 || first != next || last > _last || last < first || last - first != count - 1 || !tokens.Add(entry[2])) throw Invalid();
                reservations.Add(new(entry[1], entry[2], count, first, last)); next = (UInt128)last + 1;
            }
        }
        return (expected, next, reservations);
    }
    public AddressAllocationSnapshot Snapshot()
    {
        var state = Load();
        return new(state.Expected.Generation, state.Next > _last ? null : (ulong)state.Next, state.Reservations.ToArray());
    }
    /// <summary>One CAS attempt. After conflict or uncertain I/O, retry the exact
    /// original author/token/count. Retained matching requests publish nothing.</summary>
    public AddressReservation Reserve(string author, string token, ulong count)
    {
        Identity(author); Identity(token); if (count == 0) throw Invalid();
        var state = Load(); var retained = state.Reservations.Find(v => v.Token == token);
        if (retained is not null) return retained.Author == author && retained.Count == count ? retained : throw Invalid();
        UInt128 end = state.Next + count - 1;
        if (end > _last || state.Expected.Generation == ulong.MaxValue) throw Invalid();
        var result = new AddressReservation(author, token, count, (ulong)state.Next, (ulong)end);
        var entries = state.Expected.Entries.ToList(); if (entries.Count == 0) entries.Add(_header);
        entries.Add(JsonSerializer.Serialize(new[] { "reserve-addresses1", author, token, Text(count), Text(result.First), Text(result.Last) }));
        Admit(entries);
        bool committed;
        try { committed = _store.CompareExchange([_key], [state.Expected], [entries]); }
        catch (Exception cause) { throw new IOException("Indeterminate address reservation; retry the original author/token/count.", cause); }
        if (!committed) throw new InvalidOperationException("Conflict: address reservations changed; retry the original request.");
        return result;
    }
}
