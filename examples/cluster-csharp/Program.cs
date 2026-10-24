using Foundation.Data.Doublets.Cli.Engine;

if (args.Length != 2) throw new ArgumentException("usage: ClusterExample endpoints-comma-separated prefix/");
using var store = new EtcdJournalStore(args[0].Split(',').Select(value => new Uri(value)), args[1]);
var engine = new ClusterEngine<uint>(store, new() { RequireDurableFlush = true });
using var transaction = engine.Begin("a", "b");
foreach (var key in new[] { "a", "b" })
{
    var shard = transaction.Shard(key);
    if (shard.GetLink(1) is null) shard.CreateLink(0, 0);
    shard.SetName(1, "C# coordinated 雪");
}
transaction.Commit();
Console.WriteLine("PASS C# HTTP: both shard journals committed atomically");
