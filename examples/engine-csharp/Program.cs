using Foundation.Data.Doublets.Cli.Engine;

// dotnet run --project examples/engine-csharp -- [journal-path] [--delta] [--read-set] [--exit]
var path = args.FirstOrDefault(arg => !arg.StartsWith("--", StringComparison.Ordinal));
using var file = path is not null ? new FileCommitLog(path) : null;
ICommitLog log = file ?? (ICommitLog)new MemoryCommitLog();
if (args.Contains("--delta")) log = new DeltaCommitLog(log);
var engine = new LinksEngine<ulong>(log, new()
{
    HistoryLimit = null,
    SerializableValidation = args.Contains("--read-set") ? SerializableValidation.ReadSet : SerializableValidation.Revision,
    RequireDurableFlush = file is not null
});
using var session = engine.Begin();
var id = session.CreateLink(0, 0);
session.SetName(id, "hello 雪");
var before = engine.Snapshot();
var revision = session.Commit();
Console.WriteLine($"Committed {revision}: {before.Rows.Count} -> {engine.Snapshot().Rows.Count} rows");
using var rolledBack = engine.Begin();
rolledBack.CreateLink(0, 0);
if (args.Contains("--exit")) Environment.Exit(0); // Recovery probe: bypass disposal.
rolledBack.Rollback();

var fork = engine.Fork(new MemoryCommitLog());
using var edit = fork.Engine.Begin();
edit.SetName(id, "edited independently");
edit.Commit();
Console.WriteLine($"Merged revision {fork.Merge()}");
// Persist this with application metadata to resume the same branch later.
Console.WriteLine($"Branch merge base {fork.MergeBaseRecord}");

if (args.Contains("--read-set"))
{
    using var seed = engine.Begin(); var other = seed.CreateLink(0, 0); seed.Commit();
    using var one = engine.Begin(); using var two = engine.Begin();
    one.SetName(id, "profile one"); two.SetName(other, "profile two");
    one.Commit(); two.Commit();
    Console.WriteLine("Independent serializable point edits both committed");
}
