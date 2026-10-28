using System.Globalization;
using System.Text.Json;
using Foundation.Data.Doublets.Cli;
using Foundation.Data.Doublets.Cli.Engine;

const string alice = "00000000000000000000000000000001";
using var document = JsonDocument.Parse(File.ReadAllText("docs/protocol/causal-query-v1.json"));
int passed = 0;
foreach (var scenario in document.RootElement.GetProperty("cases").EnumerateArray())
{
    var store = new Store();
    var batch = new CausalBatch(store, "replica", "batches", alice);
    var seed = scenario.GetProperty("seed").EnumerateArray().Select(row => new CausalBatchEdit(
        Number(row, "index"), [], new(Number(row, "source"), Number(row, "target"), Name(row)))).ToArray();
    if (seed.Length > 0) batch.Write(Token(10), 0, seed);
    using var session = new CausalSession(batch, Number(scenario, "first"), Number(scenario, "last"), CausalConflictPolicy.MaxDot);
    var links = session.QueryLinks;
    foreach (var query in scenario.GetProperty("queries").EnumerateArray())
    {
        Exception? error = null;
        try { AdvancedMixedQueryProcessor<UInt128>.ProcessQuery(links, query.GetProperty("text").GetString()!); }
        catch (Exception exception) { error = exception; }
        if ((error is not null) != query.GetProperty("error").GetBoolean())
            throw new InvalidOperationException($"Unexpected query outcome: {scenario.GetProperty("name")}", error);
    }
    var actual = session.Rows().Select(row => (row.Link.Index, row.Link.Source, row.Link.Target, row.Name)).ToArray();
    var expected = scenario.GetProperty("rows").EnumerateArray().Select(row =>
        (Number(row, "index"), Number(row, "source"), Number(row, "target"), Name(row))).ToArray();
    if (!actual.SequenceEqual(expected)) throw new InvalidOperationException($"Rows differ: {scenario.GetProperty("name")}");
    UInt128 allocated = 0;
    links.Create(null, (_, row) => { allocated = row![0]; return links.Constants.Continue; });
    if (allocated != Number(scenario, "next")) throw new InvalidOperationException("Reservation cursor changed.");
    links.Delete([allocated], null);
    var prepared = session.Prepare(Token(100));
    if (prepared.Edits.Count != scenario.GetProperty("edits").GetInt32()) throw new InvalidOperationException("Wrong private edits.");
    try { links.Count(null); throw new InvalidOperationException("Prepared query reads were admitted."); }
    catch (InvalidOperationException error) when (error.Message != "Prepared query reads were admitted.") { }
    var committed = session.Commit(Token(100)).Select(version => version.Encode()).ToArray();
    var retried = session.Commit(Token(100)).Select(version => version.Encode()).ToArray();
    if (!committed.SequenceEqual(retried)) throw new InvalidOperationException("Retry differs.");
    Console.WriteLine($"PASS {scenario.GetProperty("name").GetString()}");
    passed++;
}
if (passed != 16) throw new InvalidOperationException("Missing shared query vectors.");
Console.WriteLine("PASS 16 shared full-width checked C# causal query vectors.");

static ulong Number(JsonElement row, string key) => ulong.Parse(row.GetProperty(key).GetString()!, CultureInfo.InvariantCulture);
static string? Name(JsonElement row) => row.GetProperty("name").GetString();
static string Token(int value) => value.ToString("x32", CultureInfo.InvariantCulture);

sealed class Store : IAtomicJournalStore
{
    private readonly Dictionary<string, JournalVersion> _values = new(StringComparer.Ordinal);
    public bool SupportsDurableFlush => true;
    public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys) => keys.Select(key =>
        _values.TryGetValue(key, out var value) ? value with { Entries = value.Entries.ToArray() } : new JournalVersion(0, [])).ToArray();
    public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected,
        IReadOnlyList<IReadOnlyList<string>?> updates)
    {
        if (keys.Where((key, i) => _values.GetValueOrDefault(key)?.Generation != expected[i].Generation &&
            !(_values.GetValueOrDefault(key) is null && expected[i].Generation == 0)).Any()) return false;
        for (var i = 0; i < keys.Count; i++) _values[keys[i]] = new(expected[i].Generation + 1, updates[i]!.ToArray());
        return true;
    }
}
