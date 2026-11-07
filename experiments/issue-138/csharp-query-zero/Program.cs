// Existing uint API, at most two private rows per independent control.
using Foundation.Data.Doublets.Cli;
using Foundation.Data.Doublets.Cli.Engine;
using Platform.Data.Doublets;

int failed = 0;
foreach (var (name, seed, query, expected) in new[]
{
    ("zero-swap", new[] { new Link<uint>(1, 1, 1), new Link<uint>(2, 1, 0) },
        "((2: $s $t)) ((2: $t $s))", new[] { new Link<uint>(1, 1, 1), new Link<uint>(2, 0, 1) }),
    ("zero-equality", new[] { new Link<uint>(1, 0, 1) },
        "(($i: $x $x)) ()", new[] { new Link<uint>(1, 0, 1) }),
    ("zero-join", new[] { new Link<uint>(1, 0, 1), new Link<uint>(2, 1, 1) },
        "((1: $x *) (2: $x *)) ((1: $x 1))", new[] { new Link<uint>(1, 0, 1), new Link<uint>(2, 1, 1) }),
})
{
    var engine = new LinksEngine<uint>(new MemoryCommitLog());
    using var tx = engine.Begin();
    foreach (var row in seed) { tx.EnsureCreated(row.Index); tx.UpdateLink(row.Index, row.Source, row.Target); }
    AdvancedMixedQueryProcessor.ProcessQuery(tx, query);
    var rows = new List<Link<uint>>();
    tx.Each(null, raw => { rows.Add(new(raw)); return tx.Constants.Continue; });
    bool pass = rows.SequenceEqual(expected) && engine.Snapshot().Rows.Count == 0;
    if (!pass) failed++;
    Console.WriteLine($"{name}: {(pass ? "PASS" : "FAIL")}; private={string.Join(",", rows)}; expected={string.Join(",", expected)}; published={engine.Snapshot().Rows.Count}");
}
Console.WriteLine($"Finite zero-reference controls: {3 - failed} passed; {failed} failed.");
return failed == 0 ? 0 : 1;
