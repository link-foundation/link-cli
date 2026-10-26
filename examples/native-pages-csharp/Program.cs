using Foundation.Data.Doublets.Cli.Engine;

// dotnet run --project examples/native-pages-csharp -- <database> [--delta] [--flat] [--snapshot] [--references] [--indexed-constraints]
var path = args.FirstOrDefault(arg => !arg.StartsWith("--", StringComparison.Ordinal)) ?? throw new ArgumentException("Provide a dedicated Native pages database path.");
using var device = new FilePageDevice(path);
var physical = new PagedCommitLog(device);
ICommitLog log = args.Contains("--delta") ? new DeltaCommitLog(physical) : physical;
var engine = new LinksEngine<ulong>(log, new()
{
    Isolation = Foundation.Data.Doublets.Cli.Engine.IsolationLevel.Snapshot,
    HistoryLimit = null,
    RequireDurableFlush = true,
    StateStructure = args.Contains("--flat") ? StateStructure.Flat : StateStructure.Shared,
    RevisionEncoding = args.Contains("--snapshot") ? RevisionEncoding.Snapshot : RevisionEncoding.Changes,
    ConstraintValidation = args.Contains("--indexed-constraints") ? ConstraintValidation.Indexed : ConstraintValidation.Scan,
    Constraints = args.Contains("--references") || args.Contains("--indexed-constraints") ? new[] { EngineConstraint.References } : Array.Empty<EngineConstraint>()
});
static void Require(bool condition) { if (!condition) throw new InvalidDataException("Native pages interoperability invariant failed."); }
var before = engine.Snapshot(); Require(before.Revision == (ulong)before.Rows.Count);
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
Console.WriteLine($"Native pages group: {before.Revision} -> {engine.Snapshot().Revision} revisions/rows; pinned predecessor and exact prior names verified");
