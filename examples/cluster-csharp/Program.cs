using Foundation.Data.Doublets.Cli.Engine;
if (args is ["--native-lsm-session", var sessionAction, var sessionDirectory, var sessionMirrors]) { CausalSessionInterop.Run(sessionAction, sessionDirectory, sessionMirrors); return; }

if (args is ["--native-lsm-batch", var batchAction, var batchDirectory, var batchMirrors]) { CausalBatchInterop.Run(batchAction, batchDirectory, batchMirrors); return; }
if (args is ["--native-lsm-merkle", var merkleAction, var merkleDirectory, var merkleMirrors, var merkleMessages]) { CausalMerkleInterop.Run(merkleAction, merkleDirectory, merkleMirrors, merkleMessages); return; }
if (args is ["--native-lsm-causal", var causalAction, var causalDirectory, var causalMirrors]) { CausalReplicaInterop.Run(causalAction, causalDirectory, causalMirrors); return; }
if (args is ["--native-lsm", var mode]) { NativeLsmExample.Run(mode); return; }
if (args is ["--native-lsm-files", var action, var directory, var mirrors]) { NativeLsmInterop.Run(action, directory, mirrors); return; }
if (args is ["--native-lsm-chunks", var chunkAction, var chunkDirectory, var chunkMirrors]) { ChunkInterop.Run(chunkAction, chunkDirectory, chunkMirrors); return; }
if (args is ["--native-lsm-reclaiming-chunks", var reclaimAction, var reclaimDirectory, var reclaimMirrors]) { ChunkInterop.Run(reclaimAction, reclaimDirectory, reclaimMirrors, true); return; }

var cache = args.Contains("--cache"); var checkpoint = args.Contains("--checkpoint"); args = args.Where(value => value != "--cache" && value != "--checkpoint").ToArray();
if ((args.Length != 2 && args.Length != 4) || (cache && args.Length != 4)) throw new ArgumentException("usage: ClusterExample catalog-endpoints prefix/ [east-endpoints west-endpoints [--cache]] [--checkpoint]");
using var store = new EtcdJournalStore(args[0].Split(',').Select(value => new Uri(value)), args[1]);
using var east = args.Length == 4 ? new EtcdJournalStore(args[2].Split(',').Select(value => new Uri(value)), args[1]) : null;
using var west = args.Length == 4 ? new EtcdJournalStore(args[3].Split(',').Select(value => new Uri(value)), args[1]) : null;
IAtomicJournalStore journals = args.Length == 4 ? new PartitionedJournalStore(store, new Dictionary<string, IAtomicJournalStore> { ["east"] = east!, ["west"] = west! }, key => key == "a" ? "east" : "west", cacheBytes: cache ? 65536 : 0) : store;
var engine = new ClusterEngine<uint>(journals, new() { RequireDurableFlush = true }, journalCheckpointInterval: checkpoint ? 1ul : null);
using var transaction = engine.Begin("a", "b");
foreach (var key in new[] { "a", "b" })
{
    var shard = transaction.Shard(key);
    if (shard.GetLink(1) is null) shard.CreateLink(0, 0);
    shard.SetName(1, "C# coordinated 雪");
}
transaction.Commit();
for (int i = 0; i < 2; i++)
{
    using var read = engine.Begin("a", "b");
    foreach (var key in new[] { "a", "b" }) if (read.Shard(key).GetLink(1) is null) throw new InvalidDataException("Missing committed link.");
}
Console.WriteLine("PASS C# HTTP: both shard journals committed atomically");
