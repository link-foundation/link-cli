using Foundation.Data.Doublets.Cli;
using Platform.Data.Doublets;

if (args.Length != 1) throw new ArgumentException("Provide a database directory.");
Directory.CreateDirectory(args[0]);
using var data = new NamedTypesDecorator<uint>(Path.Combine(args[0], "data"));
using var log = new NamedTypesDecorator<uint>(Path.Combine(args[0], "log"));
using var versions = new NamedTypesDecorator<uint>(Path.Combine(args[0], "versions"));
using var tx = new TransactionsDecorator(data, log);
using var vc = new VersionControlDecorator(tx, versions);
var id = vc.CreateAndUpdate(11u, 12u);
vc.SetName(id, "original 雪");
var before = vc.CurrentSequence;
using (var pending = vc.BeginTransaction())
{
    vc.SetName(id, "temporary"); pending.Rollback();
}
Verify("original 雪");
vc.SetName(id, "published 雪|\0\n");
var after = vc.CurrentSequence;
vc.Checkout(before); Verify("original 雪");
vc.Checkout(after); Verify("published 雪|\0\n");
data.Flush(); log.Flush(); versions.Flush();
Console.WriteLine($"Name rollback and versions {before} -> {after} verified for row {id}");
void Verify(string expected)
{
    if (vc.GetName(id) != expected) throw new InvalidOperationException("Unexpected name state.");
}
