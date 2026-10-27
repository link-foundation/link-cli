using System.Globalization;
using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Engine;

public sealed record CausalReplicaOptions(int MaxEntries = 4096, int MaxBytes = 8_388_608,
    bool RequireDurableFlush = false);
public sealed record CausalSnapshot(string Author, ulong LastCounter, IReadOnlyList<CausalVersion> Versions);

/// <summary>Bounded local causal owner. The caller owns its atomic store and
/// reserves its key for one replica lineage. No global atomicity is implied.</summary>
public sealed class CausalReplica
{
    private readonly IAtomicJournalStore _store;
    private readonly string _key, _author;
    private readonly CausalReplicaOptions _options;
    private static InvalidDataException Invalid() => new("Invalid causal replica state.");
    private static void Identity(string value)
    {
        if (value is null || value.Length != 32 || value.All(c => c == '0') ||
            value.Any(c => !char.IsAsciiDigit(c) && c is not (>= 'a' and <= 'f'))) throw Invalid();
    }
    private static JsonElement Raw(CausalVersion value)
    { using var document = JsonDocument.Parse(value.Encode()); return document.RootElement.Clone(); }
    private static CausalVersion Parsed(JsonElement value) => CausalVersion.Decode(JsonSerializer.Serialize(value));
    private static CausalVersion Owned(CausalVersion value) => CausalVersion.Decode(value.Encode());
    private static CausalVersion[] Values(JsonElement value)
    {
        if (value.ValueKind != JsonValueKind.Array || value.GetArrayLength() > 64) throw Invalid();
        return value.EnumerateArray().Select(Parsed).ToArray();
    }
    private static CausalVersion[] Ordered(IEnumerable<CausalVersion> values) => values.OrderBy(v => v.Address)
        .ThenBy(v => v.Dot.Author, StringComparer.Ordinal).ThenBy(v => v.Dot.Counter).ToArray();
    private static CausalVersion[] Evidence(IReadOnlyList<CausalVersion> values)
    {
        CausalVersion.Merge(values, []);
        return Ordered(values.Select(Owned).DistinctBy(v => v.Dot));
    }
    private static string Descriptor(ulong address, CausalVersion[] context, CausalPayload? payload)
    {
        var dummy = CausalVersion.Advance(address, "00000000000000000000000000000001", 0, [], payload);
        return JsonSerializer.Serialize<object?[]>([address.ToString(CultureInfo.InvariantCulture), context.Select(Raw).ToArray(), Raw(dummy)[4]]);
    }
    private sealed class State(string author)
    {
        internal ulong Counter;
        internal readonly Dictionary<CausalDot, (string Text, CausalVersion Value)> History = [];
        internal readonly Dictionary<ulong, CausalVersion[]> Rows = [];
        internal readonly Dictionary<string, (string Input, CausalVersion Value)> Tokens = new(StringComparer.Ordinal);
        internal void Remember(IReadOnlyList<CausalVersion> values, bool local = false, bool update = true)
        {
            var groups = new Dictionary<ulong, List<CausalVersion>>();
            foreach (var value in values)
            {
                string text = value.Encode();
                if (History.TryGetValue(value.Dot, out var previous)) { if (previous.Text != text) throw Invalid(); }
                else if (!local && value.Dot.Author == author) throw Invalid();
                if (value.Clock.Any(item => item.Author == author && item.Counter > Counter)) throw Invalid();
                foreach (var prior in History.Values)
                    if (prior.Value.Address == value.Address && prior.Value.Clock.SequenceEqual(value.Clock) && prior.Text != text) throw Invalid();
                History[value.Dot] = (text, value);
                if (!groups.TryGetValue(value.Address, out var group)) groups[value.Address] = group = [];
                group.Add(value);
            }
            if (update) foreach (var (address, group) in groups)
                Rows[address] = CausalVersion.Merge(Rows.GetValueOrDefault(address) ?? [], group);
        }
        internal void Apply(JsonElement entry)
        {
            if (entry.ValueKind != JsonValueKind.Array || entry.GetArrayLength() < 1 || entry[0].ValueKind != JsonValueKind.String) throw Invalid();
            if (entry[0].GetString() == "causal-receive1")
            {
                if (entry.GetArrayLength() != 2) throw Invalid(); var received = Values(entry[1]);
                if (received.Length == 0 || received.Select(v => v.Dot).Distinct().Count() != received.Length ||
                    !received.Select(v => v.Encode()).SequenceEqual(Ordered(received).Select(v => v.Encode()))) throw Invalid();
                Remember(received); return;
            }
            if (entry.GetArrayLength() != 4 || entry[0].GetString() != "causal-write1" || entry[1].ValueKind != JsonValueKind.String) throw Invalid();
            string token = entry[1].GetString()!; Identity(token); if (Tokens.ContainsKey(token)) throw Invalid();
            var seen = Evidence(Values(entry[2]));
            if (JsonSerializer.Serialize(seen.Select(Raw)) != JsonSerializer.Serialize(entry[2])) throw Invalid();
            var context = CausalVersion.Merge(seen, []);
            var mutation = Parsed(entry[3]); Remember(seen, update: false);
            var expected = CausalVersion.Advance(mutation.Address, author, Counter, context, mutation.Payload);
            if (expected.Encode() != mutation.Encode()) throw Invalid();
            Counter = mutation.Dot.Counter; Remember([mutation], local: true);
            Tokens[token] = (Descriptor(mutation.Address, context, mutation.Payload), mutation);
        }
    }
    public CausalReplica(IAtomicJournalStore store, string key, string author, CausalReplicaOptions? options = null)
    {
        ArgumentNullException.ThrowIfNull(store); Identity(author); options ??= new();
        if (string.IsNullOrEmpty(key) || key.Length > 128 || key.Any(c => !char.IsAsciiLetterOrDigit(c) && c is not ('.' or '_' or '-')) ||
            options.MaxEntries is < 2 or > 4096 || options.MaxBytes is < 1 or > 8_388_608 ||
            (options.RequireDurableFlush && !store.SupportsDurableFlush)) throw Invalid();
        _store = store; _key = key; _author = author; _options = options;
    }
    private void Admit(IReadOnlyList<string> entries)
    {
        if (entries.Count > _options.MaxEntries) throw Invalid(); long bytes = 0;
        foreach (string entry in entries)
        {
            if (entry is null || entry.Length > 1_048_576 || (bytes += entry.Length) > _options.MaxBytes) throw Invalid();
            int depth = 0, separators = 0; bool quoted = false;
            foreach (char item in entry)
            {
                if (item is < ' ' or > '~' or '\\') throw Invalid();
                if (item == '"') { quoted = !quoted; continue; }
                if (quoted) continue;
                if (item is '{' or '}') throw Invalid();
                if (item == '[') { if (++depth > 5) throw Invalid(); }
                else if (item == ']') { if (--depth < 0) throw Invalid(); }
                else if (item == ',' && ++separators > 32768) throw Invalid();
            }
            if (quoted || depth != 0) throw Invalid();
        }
    }
    private (JournalVersion Expected, State State) Load()
    {
        var result = _store.Read([_key]);
        if (result is null || result.Count != 1 || result[0] is null || result[0].Entries is null) throw Invalid();
        var expected = result[0]; Admit(expected.Entries); expected = expected with { Entries = expected.Entries.ToArray() };
        var state = new State(_author);
        if (expected.Generation == 0) { if (expected.Entries.Count != 0) throw Invalid(); return (expected, state); }
        if (expected.Entries.Count == 0 || expected.Entries[0] != JsonSerializer.Serialize(new[] { "causal-replica1", _author })) throw Invalid();
        foreach (string entry in expected.Entries.Skip(1))
        {
            using var document = JsonDocument.Parse(entry); var parsed = document.RootElement;
            if (JsonSerializer.Serialize(parsed) != entry) throw Invalid(); state.Apply(parsed);
        }
        return (expected, state);
    }
    private void Publish(JournalVersion expected, object[] entry)
    {
        var entries = expected.Entries.ToList();
        if (entries.Count == 0) entries.Add(JsonSerializer.Serialize(new[] { "causal-replica1", _author }));
        entries.Add(JsonSerializer.Serialize(entry)); Admit(entries); bool committed;
        try { committed = _store.CompareExchange([_key], [expected], [entries]); }
        catch (Exception cause) { throw new IOException("Indeterminate causal publication; retry the same token after a fresh read.", cause); }
        if (!committed) throw new InvalidOperationException("Conflict: causal state changed; reread and retry the same token.");
    }
    public CausalSnapshot Snapshot()
    {
        var (_, state) = Load(); return new(_author, state.Counter, Ordered(state.Rows.Values.SelectMany(v => v)).Select(Owned).ToArray());
    }
    public CausalVersion[] Read(ulong address)
    { if (address == 0) throw Invalid(); return (Load().State.Rows.GetValueOrDefault(address) ?? []).Select(Owned).ToArray(); }
    public CausalVersion Write(string token, ulong address, IReadOnlyList<CausalVersion> observed, CausalPayload? payload)
    {
        Identity(token); var admitted = Evidence(observed); var context = CausalVersion.Merge(admitted, []); string input = Descriptor(address, context, payload);
        var (expected, state) = Load();
        state.Remember(admitted, update: false);
        if (state.Tokens.TryGetValue(token, out var prior)) { if (prior.Input != input) throw Invalid(); return Owned(prior.Value); }
        var mutation = CausalVersion.Advance(address, _author, state.Counter, context, payload);
        object[] entry = ["causal-write1", token, admitted.Select(Raw).ToArray(), Raw(mutation)];
        using var document = JsonDocument.Parse(JsonSerializer.Serialize(entry)); state.Apply(document.RootElement);
        Publish(expected, entry); return Owned(mutation);
    }
    public bool Receive(IReadOnlyList<CausalVersion> incoming)
    {
        ArgumentNullException.ThrowIfNull(incoming); if (incoming.Count > 64) throw Invalid();
        var admitted = incoming.Select(Owned).ToArray(); var (expected, state) = Load();
        var known = state.History.Keys.ToHashSet(); state.Remember(admitted);
        var added = Ordered(admitted.Where(v => !known.Contains(v.Dot)).DistinctBy(v => v.Dot));
        if (added.Length == 0) return false;
        Publish(expected, ["causal-receive1", added.Select(Raw).ToArray()]); return true;
    }
}
