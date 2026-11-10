using System.Globalization;
using System.Text;
using System.Text.Json;
namespace Foundation.Data.Doublets.Cli.Engine;

public sealed record CausalDot(string Author, ulong Counter);
public sealed record CausalPayload(ulong Source, ulong Target, string? Name);

/// <summary>Selected causal semantics. Performs no I/O, owner fencing or replication.
/// Mutable caller collections are revalidated and copied at every boundary.</summary>
public sealed record CausalVersion(ulong Address, CausalDot Dot,
    IReadOnlyList<CausalDot> Clock, CausalPayload? Payload)
{
    private static readonly UTF8Encoding Utf8 = new(false, true);
    private static InvalidDataException Invalid() => new("Invalid causal version.");
    private static string Number(ulong value) => value.ToString(CultureInfo.InvariantCulture);
    private static void Author(string value)
    {
        if (value is null || value.Length != 32 || value.All(c => c == '0') ||
            value.Any(c => !char.IsAsciiDigit(c) && c is not (>= 'a' and <= 'f'))) throw Invalid();
    }
    private static ulong Counter(JsonElement value, bool positive = true)
    {
        if (value.ValueKind != JsonValueKind.String) throw Invalid();
        string text = value.GetString()!;
        if (text.Length is < 1 or > 20 || !ulong.TryParse(text, NumberStyles.None,
            CultureInfo.InvariantCulture, out ulong count) || Number(count) != text || (positive && count == 0)) throw Invalid();
        return count;
    }
    private static void Shape(JsonElement value, int count)
    { if (value.ValueKind != JsonValueKind.Array || value.GetArrayLength() != count) throw Invalid(); }
    private static string String(JsonElement value)
    { if (value.ValueKind != JsonValueKind.String) throw Invalid(); return value.GetString()!; }
    private static CausalDot DotValue(JsonElement value)
    { Shape(value, 2); return new(String(value[0]), Counter(value[1])); }
    public string Encode()
    {
        if (Address == 0 || Dot is null || Dot.Counter == 0 || Clock is null || Clock.Count is < 1 or > 64) throw Invalid();
        Author(Dot.Author); string previous = ""; bool found = false;
        foreach (var item in Clock)
        {
            if (item is null || item.Counter == 0) throw Invalid(); Author(item.Author);
            if (string.CompareOrdinal(item.Author, previous) <= 0) throw Invalid(); previous = item.Author;
            if (item.Author == Dot.Author) { if (item.Counter != Dot.Counter) throw Invalid(); found = true; }
        }
        if (!found) throw Invalid();
        string? name = null;
        if (Payload?.Name is string value)
        {
            if (value.Length > 4096 || Utf8.GetByteCount(value) > 4096) throw Invalid();
            name = Convert.ToHexStringLower(Utf8.GetBytes(value));
        }
        return JsonSerializer.Serialize<object?[]>(["causal-version1", Number(Address),
            new[] { Dot.Author, Number(Dot.Counter) },
            Clock.Select(item => new[] { item.Author, Number(item.Counter) }).ToArray(),
            Payload is null ? null : new object?[] { Number(Payload.Source), Number(Payload.Target), name }]);
    }
    public static CausalVersion Decode(string wire)
    {
        ArgumentNullException.ThrowIfNull(wire);
        if (wire.Length > 65536 || wire.Any(c => c is < ' ' or > '~')) throw Invalid();
        using var document = JsonDocument.Parse(wire); var root = document.RootElement; Shape(root, 5);
        if (String(root[0]) != "causal-version1") throw Invalid();
        if (root[3].ValueKind != JsonValueKind.Array || root[3].GetArrayLength() is < 1 or > 64) throw Invalid();
        var clock = root[3].EnumerateArray().Select(DotValue).ToArray(); CausalPayload? row = null;
        if (root[4].ValueKind != JsonValueKind.Null)
        {
            var value = root[4]; Shape(value, 3); string? name = null;
            if (value[2].ValueKind != JsonValueKind.Null)
            {
                string hex = String(value[2]);
                if (hex.Length > 8192 || hex.Length % 2 != 0 ||
                    hex.Any(c => !char.IsAsciiDigit(c) && c is not (>= 'a' and <= 'f'))) throw Invalid();
                name = Utf8.GetString(Convert.FromHexString(hex));
            }
            row = new(Counter(value[0], false), Counter(value[1], false), name);
        }
        var result = new CausalVersion(Counter(root[1]), DotValue(root[2]), clock, row);
        if (result.Encode() != wire) throw Invalid(); return result;
    }
    private static CausalVersion[] Owned(IReadOnlyList<CausalVersion> values)
    {
        ArgumentNullException.ThrowIfNull(values); if (values.Count > 64) throw Invalid();
        return values.Select(value => value is null ? throw Invalid() : Decode(value.Encode())).ToArray();
    }
    private static bool Dominates(CausalVersion left, CausalVersion right) =>
        right.Clock.All(item => left.Clock.Any(other => other.Author == item.Author && other.Counter >= item.Counter));
    public static CausalVersion[] Merge(IReadOnlyList<CausalVersion> left, IReadOnlyList<CausalVersion> right)
    {
        var values = Owned(left).Concat(Owned(right)).ToArray();
        var unique = new List<CausalVersion>(); ulong? address = values.FirstOrDefault()?.Address;
        foreach (var value in values)
        {
            if (value.Address != address) throw Invalid(); bool duplicate = false;
            foreach (var previous in unique)
            {
                if (previous.Dot == value.Dot || previous.Clock.SequenceEqual(value.Clock))
                { if (previous.Encode() != value.Encode()) throw Invalid(); duplicate = true; }
            }
            if (!duplicate) unique.Add(value);
        }
        var result = unique.Where(value => !unique.Any(other => !ReferenceEquals(other, value) && Dominates(other, value)))
            .OrderBy(value => value.Dot.Author, StringComparer.Ordinal).ThenBy(value => value.Dot.Counter).ToArray();
        if (result.Length > 64) throw Invalid(); return result;
    }
    /// <summary>Build a mutation only. The caller must atomically persist its
    /// counter and mutation and fence duplicate author owners before using it.</summary>
    public static CausalVersion Advance(ulong address, string author, ulong lastCounter,
        IReadOnlyList<CausalVersion> observed, CausalPayload? payload)
    {
        if (address == 0) throw Invalid(); Author(author); var values = Owned(observed);
        _ = Merge(values, []); var clock = new SortedDictionary<string, ulong>(StringComparer.Ordinal);
        foreach (var value in values)
        {
            if (value.Address != address) throw Invalid();
            foreach (var item in value.Clock)
                clock[item.Author] = Math.Max(clock.GetValueOrDefault(item.Author), item.Counter);
        }
        ulong next = Math.Max(lastCounter, clock.GetValueOrDefault(author));
        if (next == ulong.MaxValue) throw Invalid(); clock[author] = ++next;
        return Decode(new CausalVersion(address, new(author, next),
            clock.Select(item => new CausalDot(item.Key, item.Value)).ToArray(), payload).Encode());
    }
}
