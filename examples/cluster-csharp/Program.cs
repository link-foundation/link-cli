using Foundation.Data.Doublets.Cli.Engine;

if (args.Length != 2 && args.Length != 4) throw new ArgumentException("usage: ClusterExample catalog-endpoints prefix/ [east-endpoints west-endpoints]");
using var store = new EtcdJournalStore(args[0].Split(',').Select(value => new Uri(value)), args[1]);
using var east = args.Length == 4 ? new EtcdJournalStore(args[2].Split(',').Select(value => new Uri(value)), args[1]) : null;
using var west = args.Length == 4 ? new EtcdJournalStore(args[3].Split(',').Select(value => new Uri(value)), args[1]) : null;
IAtomicJournalStore journals = args.Length == 4 ? new PartitionedJournalStore(store, new Dictionary<string, IAtomicJournalStore> { ["east"] = east!, ["west"] = west! }, key => key == "a" ? "east" : "west") : store;
var engine = new ClusterEngine<uint>(journals, new() { RequireDurableFlush = true });
using var transaction = engine.Begin("a", "b");
foreach (var key in new[] { "a", "b" })
{
    var shard = transaction.Shard(key);
    if (shard.GetLink(1) is null) shard.CreateLink(0, 0);
    shard.SetName(1, "C# coordinated 雪");
}
transaction.Commit();
Console.WriteLine("PASS C# HTTP: both shard journals committed atomically");
