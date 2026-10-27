using System.Globalization;
using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Engine;

public sealed record CausalBatchEdit(ulong Address, IReadOnlyList<CausalVersion> Observed, CausalPayload? Payload);
public sealed record CausalBatchSnapshot(ulong Generation, string Author, ulong LastCounter, IReadOnlyList<CausalVersion> Versions);

/// <summary>Selected bounded complete-journal batches. The caller owns the
/// coherent store and reserves two distinct keys. Remote leaves do not retain
/// local group atomicity. See ENGINE-CAUSAL-BATCHES.md for costs and limits.</summary>
public sealed class CausalBatch
{
    private readonly IAtomicJournalStore _store;
    private readonly string[] _keys;
    private readonly string _author;
    private readonly CausalReplicaOptions _options;
    private static InvalidDataException Invalid() => new("Invalid causal batch state.");
    private static string Number(ulong value) => value.ToString(CultureInfo.InvariantCulture);
    private static string Wire(object? value) => JsonSerializer.Serialize(value);
    private static JsonElement Parsed(string value)
    { using var document = JsonDocument.Parse(value); return document.RootElement.Clone(); }
    private static JsonElement Raw(CausalVersion value) => Parsed(value.Encode());
    private static CausalVersion Owned(CausalVersion value) => CausalVersion.Decode(value.Encode());
    private static UInt128 Identity(string value)
    {
        if (value is null || value.Length != 32 || value.All(c => c == '0') ||
            value.Any(c => !char.IsAsciiDigit(c) && c is not (>= 'a' and <= 'f'))) throw Invalid();
        return UInt128.Parse(value, NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture);
    }
    private static ulong Number(JsonElement value)
    {
        if (value.ValueKind != JsonValueKind.String) throw Invalid(); string text = value.GetString()!;
        if (text.Length is < 1 or > 20 || !ulong.TryParse(text, NumberStyles.None,
            CultureInfo.InvariantCulture, out ulong number) || Number(number) != text) throw Invalid();
        return number;
    }
    private static void Range(UInt128 token, int count)
    { if (UInt128.MaxValue - token < (UInt128)Math.Max(count - 1, 0)) throw Invalid(); }
    private static string Child(UInt128 token, int index) => (token + (UInt128)index).ToString("x32", CultureInfo.InvariantCulture);
    private static (CausalBatchEdit[] Edits, JsonElement Encoded) Normalize(IReadOnlyList<CausalBatchEdit> edits)
    {
        if (edits is null || edits.Count > 64) throw Invalid(); ulong previous = 0;
        var owned = new List<CausalBatchEdit>(); var encoded = new List<object[]>();
        foreach (var edit in edits)
        {
            if (edit is null || edit.Address <= previous || edit.Observed is null || edit.Observed.Count > 64) throw Invalid();
            previous = edit.Address;
            var observed = edit.Observed.Select(Owned).ToArray(); var context = CausalVersion.Merge(observed, []);
            var dummy = Owned(CausalVersion.Advance(edit.Address, "00000000000000000000000000000001", 0, [], edit.Payload));
            owned.Add(new(edit.Address, observed, dummy.Payload));
            encoded.Add([Number(edit.Address), context.Select(Raw).ToArray(), Raw(dummy)[4]]);
        }
        return (owned.ToArray(), Parsed(Wire(encoded)));
    }
    private CausalBatchEdit[] ParseEdits(JsonElement edits)
    {
        if (edits.ValueKind != JsonValueKind.Array || edits.GetArrayLength() > 64) throw Invalid();
        return edits.EnumerateArray().Select(edit =>
        {
            if (edit.ValueKind != JsonValueKind.Array || edit.GetArrayLength() != 3 ||
                edit[1].ValueKind != JsonValueKind.Array || edit[1].GetArrayLength() > 64) throw Invalid();
            ulong address = Number(edit[0]);
            var observed = edit[1].EnumerateArray().Select(value => CausalVersion.Decode(Wire(value))).ToArray();
            var dummy = CausalVersion.Decode(Wire(new object[] { "causal-version1", edit[0],
                new[] { _author, "1" }, new[] { new[] { _author, "1" } }, edit[2] }));
            return new CausalBatchEdit(address, observed, dummy.Payload);
        }).ToArray();
    }
    private sealed class StagingStore(JournalVersion value) : IAtomicJournalStore
    {
        internal JournalVersion Value = new(value.Generation == 0 ? 0UL : 1UL, value.Entries.ToArray());
        public bool SupportsDurableFlush => true;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys) => [Value with { Entries = Value.Entries.ToArray() }];
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
        { Value = new(expected[0].Generation + 1, updates[0]!.ToArray()); return true; }
    }
    private sealed record Receipt(string Request, CausalVersion[] Versions);
    private sealed record Loaded(JournalVersion[] Expected, StagingStore Staging, CausalReplica Owner,
        CausalSnapshot Snapshot, Dictionary<string, Receipt> Receipts);
    public CausalBatch(IAtomicJournalStore store, string ownerKey, string receiptKey, string author, CausalReplicaOptions? options = null)
    {
        options ??= new(); _ = new CausalReplica(store, ownerKey, author, options);
        if (string.IsNullOrEmpty(receiptKey) || receiptKey.Length > 128 || receiptKey == ownerKey ||
            receiptKey.Any(c => !char.IsAsciiLetterOrDigit(c) && c is not ('.' or '_' or '-'))) throw Invalid();
        _store = store; _keys = [ownerKey, receiptKey]; _author = author; _options = options;
    }
    private void Admit(IReadOnlyList<string> entries)
    {
        if (entries is null || entries.Count > _options.MaxEntries) throw Invalid(); long bytes = 0;
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
                if (item == '[') { if (++depth > 7) throw Invalid(); }
                else if (item == ']') { if (--depth < 0) throw Invalid(); }
                else if (item == ',' && ++separators > 32768) throw Invalid();
            }
            if (quoted || depth != 0) throw Invalid();
        }
    }
    private Loaded Load()
    {
        var read = _store.Read(_keys.ToArray());
        if (read is null || read.Count != 2) throw Invalid();
        var expected = read.Select(value =>
        { if (value is null) throw Invalid(); Admit(value.Entries); return value with { Entries = value.Entries.ToArray() }; }).ToArray();
        var staging = new StagingStore(expected[0]); var owner = new CausalReplica(staging, _keys[0], _author, _options);
        var snapshot = owner.Snapshot();
        var writes = new Dictionary<string, (string Request, CausalVersion Value)>(StringComparer.Ordinal);
        foreach (string text in expected[0].Entries.Skip(1))
        {
            var entry = Parsed(text);
            if (entry[0].GetString() != "causal-write1") continue;
            var value = CausalVersion.Decode(Wire(entry[3]));
            var observed = entry[2].EnumerateArray().Select(v => CausalVersion.Decode(Wire(v))).ToArray();
            var encoded = Normalize([new(value.Address, observed, value.Payload)]).Encoded[0];
            writes.Add(entry[1].GetString()!, (Wire(encoded), value));
        }
        var receipts = new Dictionary<string, Receipt>(StringComparer.Ordinal); var used = new HashSet<string>(StringComparer.Ordinal);
        var metadata = expected[1];
        if (metadata.Generation == 0) { if (metadata.Entries.Count != 0) throw Invalid(); }
        else
        {
            if (expected[0].Generation == 0 || metadata.Entries.Count == 0 || metadata.Entries[0] != Wire(new[] { "causal-batches1", _keys[0], _author })) throw Invalid();
            foreach (string text in metadata.Entries.Skip(1))
            {
                var entry = Parsed(text);
                if (Wire(entry) != text || entry.ValueKind != JsonValueKind.Array || entry.GetArrayLength() != 5 ||
                    entry[0].ValueKind != JsonValueKind.String || entry[0].GetString() != "causal-batch1" || entry[1].ValueKind != JsonValueKind.String) throw Invalid();
                string token = entry[1].GetString()!; var baseToken = Identity(token); ulong generation = Number(entry[2]), start = Number(entry[3]);
                if (receipts.ContainsKey(token) || start > snapshot.LastCounter) throw Invalid();
                var edits = ParseEdits(entry[4]); var normalized = Normalize(edits).Encoded; Range(baseToken, edits.Length);
                if (Wire(normalized) != Wire(entry[4])) throw Invalid();
                var versions = new List<CausalVersion>();
                for (int index = 0; index < edits.Length; index++)
                {
                    string child = Child(baseToken, index);
                    if (!used.Add(child) || !writes.TryGetValue(child, out var found) ||
                        found.Value.Dot.Counter != checked(start + (ulong)index + 1) || found.Request != Wire(normalized[index])) throw Invalid();
                    versions.Add(Owned(found.Value));
                }
                receipts.Add(token, new(Wire(new object[] { Number(generation), normalized }), versions.ToArray()));
            }
        }
        return new(expected, staging, owner, snapshot, receipts);
    }
    public CausalBatchSnapshot Snapshot()
    { var loaded = Load(); return new(loaded.Expected[0].Generation, _author, loaded.Snapshot.LastCounter, loaded.Snapshot.Versions); }
    public CausalVersion[] Write(string token, ulong generation, IReadOnlyList<CausalBatchEdit> edits)
    {
        var baseToken = Identity(token); var (admitted, encoded) = Normalize(edits); Range(baseToken, admitted.Length);
        string request = Wire(new object[] { Number(generation), encoded }); var loaded = Load();
        loaded.Receipts.TryGetValue(token, out var receipt);
        if (receipt is not null) { if (receipt.Request != request) throw Invalid(); }
        else if (loaded.Expected[0].Generation != generation) throw new InvalidOperationException("Conflict: stale causal batch base.");
        var result = new List<CausalVersion>();
        for (int index = 0; index < admitted.Length; index++)
        {
            var edit = admitted[index]; var value = loaded.Owner.Write(Child(baseToken, index), edit.Address, edit.Observed, edit.Payload);
            ulong counter = receipt is not null ? receipt.Versions[index].Dot.Counter : checked(loaded.Snapshot.LastCounter + (ulong)index + 1);
            if (value.Dot.Counter != counter) throw new InvalidDataException("Causal batch child-token collision.");
            result.Add(value);
        }
        if (receipt is not null) return result.ToArray();
        var ownerEntries = loaded.Staging.Value.Entries.ToList();
        if (ownerEntries.Count == 0) ownerEntries.Add(Wire(new[] { "causal-replica1", _author }));
        var entries = loaded.Expected[1].Entries.ToList();
        if (entries.Count == 0) entries.Add(Wire(new[] { "causal-batches1", _keys[0], _author }));
        entries.Add(Wire(new object[] { "causal-batch1", token, Number(generation), Number(loaded.Snapshot.LastCounter), encoded }));
        Admit(entries); bool committed;
        try { committed = _store.CompareExchange(_keys.ToArray(), loaded.Expected, [ownerEntries, entries]); }
        catch (Exception cause) { throw new IOException("Indeterminate causal batch publication; reread and retry the original request.", cause); }
        if (!committed) throw new InvalidOperationException("Conflict: causal batch state changed.");
        return result.ToArray();
    }
}
