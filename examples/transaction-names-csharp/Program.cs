using Foundation.Data.Doublets.Cli;
using Platform.Data.Doublets;

if (args.Length is < 1 or > 2 || (args.Length == 2 && args[1] != "--row-image"))
    throw new ArgumentException("Provide a database directory and optional --row-image.");
var policy = args.Length == 2 ? RowUndoPolicy.WriteAheadImage : RowUndoPolicy.Observed;
Directory.CreateDirectory(args[0]);
using var data = new NamedTypesDecorator<uint>(Path.Combine(args[0], "data"));
using var log = new NamedTypesDecorator<uint>(Path.Combine(args[0], "log"));
using var versions = new NamedTypesDecorator<uint>(Path.Combine(args[0], "versions"));
using var tx = new TransactionsDecorator(data, log, policy);
using var vc = new VersionControlDecorator(tx, versions);
var id = vc.CreateAndUpdate(11u, 12u);
vc.SetName(id, "original 雪");
var before = vc.CurrentSequence;
using (var pending = vc.BeginTransaction())
{
    vc.Update(new[] { id }, new[] { id, 21u, 22u }, null);
    vc.SetName(id, "temporary"); pending.Rollback();
}
Verify("original 雪");
if (vc.GetSource(id) != 11u || vc.GetTarget(id) != 12u) throw new InvalidOperationException("Row rollback failed.");
vc.SetName(id, "published 雪|\0\n");
var after = vc.CurrentSequence;
vc.Checkout(before); Verify("original 雪");
vc.Checkout(after); Verify("published 雪|\0\n");
data.Flush(); log.Flush(); versions.Flush();
Console.WriteLine($"Row/name rollback and versions {before} -> {after} verified for row {id}; undo={policy}");
void Verify(string expected)
{
    if (vc.GetName(id) != expected) throw new InvalidOperationException("Unexpected name state.");
}
