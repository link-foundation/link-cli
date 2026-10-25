using Foundation.Data.Doublets.Cli.Engine;

// dotnet run --project examples/group-commit-csharp -- <journal> [--delta]
var path = args.FirstOrDefault(arg => !arg.StartsWith("--", StringComparison.Ordinal))
    ?? throw new ArgumentException("Provide an exclusive journal path.");
using var file = new FileCommitLog(path);
ICommitLog log = args.Contains("--delta") ? new DeltaCommitLog(file) : file;
var engine = new LinksEngine<ulong>(log, new() { Isolation = IsolationLevel.Snapshot, HistoryLimit = null, RequireDurableFlush = true });
using var first = engine.Begin(); ulong a = first.CreateLink(0, 0); first.SetName(a, "first independent session");
using var second = engine.Begin(); ulong b = second.CreateLink(a, 0); second.SetName(b, "second independent session");
foreach (var outcome in engine.CommitGroup(new[] { first, second }))
{
    if (outcome.Status == GroupCommitStatus.Indeterminate)
        throw new IOException($"{outcome.Error}; inspect reopened state before retrying.");
    Console.WriteLine(outcome.Status == GroupCommitStatus.Committed
        ? $"Acknowledged revision {outcome.Revision}" : $"Known abort: {outcome.Error}; retry with a new session.");
}
