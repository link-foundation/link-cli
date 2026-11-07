// Finite existing uint API control; no causal or proposed generic APIs needed.
using System.Text.Json;
using Foundation.Data.Doublets.Cli;
using Foundation.Data.Doublets.Cli.Engine;
using Platform.Data.Doublets;

int failures = 0;
foreach (var (name, query, expected) in new[]
{
    ("anonymous-point", "(() ((1 1)))", new[] { new Link<uint>(1, 1, 1) }),
    ("explicit-point-control", "() ((1: 1 1))", new[] { new Link<uint>(1, 1, 1) }),
    ("explicit-pair-reuse-control", "() ((4: 4 4) (4 4))", new[] { new Link<uint>(4, 4, 4) }),
})
{
    var engine = new LinksEngine<uint>(new MemoryCommitLog());
    using var tx = engine.Begin();
    Exception? error = null;
    try { AdvancedMixedQueryProcessor.ProcessQuery(tx, query); }
    catch (Exception caught) { error = caught; }
    var rows = new List<Link<uint>>();
    tx.Each(null, parts => { rows.Add(new(parts)); return tx.Constants.Continue; });
    bool matches = error is null && rows.SequenceEqual(expected) && engine.Snapshot().Rows.Count == 0;
    if (!matches) failures++;
    Console.WriteLine(JsonSerializer.Serialize(new
    {
        name, query, pass = matches, error = error?.ToString(),
        rows = rows.Select(row => new { row.Index, row.Source, row.Target }),
        expected = expected.Select(row => new { row.Index, row.Source, row.Target }),
        published = engine.Snapshot().Rows.Count,
    }));
}
Console.WriteLine($"Finite query allocation controls: {3 - failures} passed; {failures} failed.");
return failures == 0 ? 0 : 1;
