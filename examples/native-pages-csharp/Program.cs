using Foundation.Data.Doublets.Cli.Engine;
using System.Text.Json;

// dotnet run --project examples/native-pages-csharp -- <database> [--mirrored] [--delta] [--flat|--sorted-runs] [--snapshot] [--references] [--indexed-constraints] [--checkpoint-fixture=<json>]
var path = args.FirstOrDefault(arg => !arg.StartsWith("--", StringComparison.Ordinal)) ?? throw new ArgumentException("Provide a dedicated Native pages database path.");
IPageDevice device = args.Contains("--mirrored") ? new FileMirroredPageDevice(path) : new FilePageDevice(path);
using var ownership = (IDisposable)device;
var physical = new PagedCommitLog(device);
ICommitLog log = args.Contains("--delta") ? new DeltaCommitLog(physical) : physical;
string? checkpoint = null;
const string fixturePrefix = "--checkpoint-fixture=";
if (args.FirstOrDefault(arg => arg.StartsWith(fixturePrefix, StringComparison.Ordinal)) is { } fixture)
{
    using var json = JsonDocument.Parse(File.ReadAllText(fixture[fixturePrefix.Length..]));
    checkpoint = json.RootElement.GetProperty("nativePageSeed").GetString()
        ?? throw new InvalidDataException("Fixture must contain nativePageSeed.");
    if (log.Read().Count == 0) { log.Append(checkpoint); log.Flush(); }
    Require(log.Read()[0] == checkpoint);
}
var engine = new LinksEngine<ulong>(log, new()
{
    Isolation = Foundation.Data.Doublets.Cli.Engine.IsolationLevel.Snapshot,
    HistoryLimit = null,
    RequireDurableFlush = true,
    StateStructure = args.Contains("--flat") ? StateStructure.Flat : args.Contains("--sorted-runs") ? StateStructure.SortedRuns : StateStructure.Shared,
    RevisionEncoding = args.Contains("--snapshot") ? RevisionEncoding.Snapshot : RevisionEncoding.Changes,
    ConstraintValidation = args.Contains("--indexed-constraints") ? ConstraintValidation.Indexed : ConstraintValidation.Scan,
    Constraints = args.Contains("--references") || args.Contains("--indexed-constraints") ? new[] { EngineConstraint.References } : Array.Empty<EngineConstraint>()
});
static void Require(bool condition) { if (!condition) throw new InvalidDataException("Native pages interoperability invariant failed."); }
var before = engine.Snapshot();
if (checkpoint is not null)
{
    Require(before.Revision >= 40 && before.Revision - 8 == (ulong)before.Rows.Count);
    Require(engine.SnapshotAt(40).Rows.Count == 32 && engine.ExportCheckpoint()[0] == checkpoint);
    Require(Enumerable.Range(33, 8).All(id => !before.Rows.ContainsKey((ulong)id)));
    bool unavailable = false;
    try { engine.SnapshotAt(39); }
    catch (KeyNotFoundException) { unavailable = true; }
    Require(unavailable);
}
else Require(before.Revision == (ulong)before.Rows.Count);
foreach (var (id, row) in before.Rows)
{
    Require(row.Link.Source == (id % 2 == 1 ? 0UL : id - 1) && row.Link.Target == 0);
    Require(row.Name == (id % 2 == 1 ? "\ufefffirst 雪\0😀" : "second"));
}
using var a = engine.Begin(); ulong first = a.CreateLink(0, 0); a.SetName(first, "\ufefffirst 雪\0😀");
using var b = engine.Begin(); ulong second = b.CreateLink(first, 0); b.SetName(second, "second");
Require(first == before.Revision + 1 && second == before.Revision + 2);
var outcomes = engine.CommitGroup(new[] { a, b });
Require(outcomes.Select(result => result.Revision).SequenceEqual(new ulong?[] { before.Revision + 1, before.Revision + 2 }));
Require(engine.Snapshot().Rows.Count == before.Rows.Count + 2 && engine.SnapshotAt(before.Revision).Rows.SequenceEqual(before.Rows));
var after = engine.Snapshot();
Console.WriteLine(checkpoint is not null
    ? $"Native checkpoint pages group: {before.Revision} -> {after.Revision} revisions; {before.Rows.Count} -> {after.Rows.Count} rows; pinned predecessor and exact prior names verified"
    : $"Native pages group: {before.Revision} -> {after.Revision} revisions/rows; pinned predecessor and exact prior names verified");
