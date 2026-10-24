using Foundation.Data.Doublets.Cli.Engine;

// dotnet run --project examples/engine-csharp -- [journal-path] [--exit]
using var file = args.Length > 0 ? new FileCommitLog(args[0]) : null;
var engine = new LinksEngine<ulong>(file ?? (ICommitLog)new MemoryCommitLog(), new()
{
    HistoryLimit = null,
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
