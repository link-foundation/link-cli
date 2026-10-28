using Foundation.Data.Doublets.Cli.Engine;
if (args.Length != 2) throw new ArgumentException("usage: endpoints-comma-separated dedicated-prefix/");
var endpoints = args[0].Split(',').Select(value => new Uri(value)).ToArray();
using var catalog = new EtcdJournalStore(endpoints, $"{args[1]}catalog/");
using var east = new EtcdJournalStore(endpoints, $"{args[1]}east/");
using var west = new EtcdJournalStore(endpoints, $"{args[1]}west/");
var chunked = Environment.GetEnvironmentVariable("RECLAMATION_CHUNKS") == "1";
var store = ReclaimingPartitionedJournalStore.Open(catalog, new Dictionary<string, IJournalMaintenanceStore> { ["east"] = east, ["west"] = west }, key => key == "a" ? "east" : "west", new(PageKeys: 1, CacheBytes: 65536), chunked ? new JournalChunkOptions(MaxRecords: 2) : null);
void Check(bool condition) { if (!condition) throw new InvalidDataException("Reclamation contract failed."); }
ulong Verify()
{
    var revisions = new List<ulong>();
    foreach (var journal in store.Read(new[] { "a", "b" }))
    {
        var log = new MemoryCommitLog(); foreach (var entry in journal.Entries) log.Append(entry);
        var local = new LinksEngine<uint>(log, new() { HistoryLimit = null });
        var current = local.Snapshot(); var revision = current.Revision;
        if (revision > 0)
        {
            Check(current.Rows.Count == 1 && current.Rows[1].Name == $"revision-{revision} 雪");
            for (ulong old = 1; old < revision; old++) Check(local.SnapshotAt(old).Rows[1].Name == $"revision-{old} 雪");
            using var probe = local.Begin(); Check(probe.CreateLink(0, 0) == checked((uint)revision + 2)); probe.Rollback();
        }
        revisions.Add(revision);
    }
    Check(revisions[0] == revisions[1]); return revisions[0];
}
var revision = Verify(); Check(revision < 32);
var retired = catalog.Read(new[] { "a", "b" }).Sum(root => root.Generation == 0 ? 0 : root.Entries[0] == "chunks2" ? int.Parse(root.Entries[3], System.Globalization.CultureInfo.InvariantCulture) : root.Entries[0] == "blob2" ? 1 : throw new InvalidDataException("Unexpected layout."));
var engine = new ClusterEngine<uint>(store, new() { HistoryLimit = null, RequireDurableFlush = true }, journalCheckpointInterval: 1);
using var stale = engine.Begin("a", "b"); using var write = engine.Begin("a", "b");
foreach (var key in new[] { "a", "b" })
{
    var shard = write.Shard(key);
    if (revision == 0) Check(shard.CreateLink(0, 0) == 1);
    var gap = shard.CreateLink(0, 0); Check(gap == checked((uint)revision + 2)); shard.DeleteLink(gap);
    shard.SetName(1, $"revision-{revision + 1} 雪");
}
write.Commit();
var collected = store.CollectGarbage() ?? throw new InvalidOperationException("Unexpected competing collector.");
Check(collected.Deleted == retired);
if (revision > 0) Check(stale.Shard("a").GetName(1) == $"revision-{revision} 雪");
bool conflicted = false;
try { stale.Commit(); } catch (InvalidOperationException error) when (error.Message.Contains("conflict", StringComparison.OrdinalIgnoreCase)) { conflicted = true; }
Check(conflicted && Verify() == revision + 1);
Console.WriteLine($"PASS C# reclamation: revision {revision + 1}, atomic checkpoints, all retained history, deleted allocator gap, owned stale conflict, {collected.Deleted} retired objects");
