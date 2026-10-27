using Foundation.Data.Doublets.Cli.Engine;

// Real native doublets files, independent of HTTP servers or SQL providers.
internal static class NativeLsmExample
{
    private static void Require(bool condition, string message)
    { if (!condition) throw new InvalidDataException(message); }
    private static FileLsmJournalStore Open(string directory, string name)
    {
        string path = Directory.CreateDirectory(Path.Combine(directory, name)).FullName;
        return FileLsmJournalStore.Open(path, new(new(new LsmOptions(256, 1048576, 64, 262144)), 128));
    }
    private static void WithStores(string directory, string mode, Action<IAtomicJournalStore, ReclaimingPartitionedJournalStore?> action)
    {
        using var catalog = Open(directory, "catalog");
        if (mode == "direct") { action(catalog, null); catalog.Compact(); return; }
        using var east = Open(directory, "east"); using var west = Open(directory, "west");
        if (mode == "partitioned")
            action(new PartitionedJournalStore(catalog, new Dictionary<string, IAtomicJournalStore> { ["east"] = east, ["west"] = west }, key => key == "a" ? "east" : "west"), null);
        else
        {
            var reclaiming = ReclaimingPartitionedJournalStore.Open(catalog, new Dictionary<string, IJournalMaintenanceStore> { ["east"] = east, ["west"] = west }, key => key == "a" ? "east" : "west");
            action(reclaiming, reclaiming);
        }
        catalog.Compact(); east.Compact(); west.Compact();
    }
    internal static void Run(string mode)
    {
        if (mode is not ("direct" or "partitioned" or "reclaiming")) throw new ArgumentException("Native LSM mode must be direct, partitioned or reclaiming.", nameof(mode));
        string directory = Directory.CreateTempSubdirectory("native-lsm-consumer-").FullName;
        try
        {
            WithStores(directory, mode, (journals, reclaiming) =>
            {
                var engine = new ClusterEngine<uint>(journals, new() { RequireDurableFlush = !OperatingSystem.IsWindows() }, journalCheckpointInterval: 1);
                for (int revision = 0; revision < 3; revision++)
                {
                    using var transaction = engine.Begin("a", "b");
                    foreach (string key in new[] { "a", "b" })
                    {
                        var shard = transaction.Shard(key); if (shard.GetLink(1) is null) shard.CreateLink(0, 0);
                        shard.SetName(1, $"revision-{revision} 雪😀");
                    }
                    transaction.Commit();
                }
                using var stale = engine.Begin("a", "b");
                using (var winner = engine.Begin("a")) { winner.Shard("a").SetName(1, "winner"); winner.Commit(); }
                stale.Shard("b").SetName(1, "lost");
                try { stale.Commit(); throw new InvalidDataException("A stale group committed."); }
                catch (InvalidOperationException error) when (error.Message.Contains("Conflict", StringComparison.Ordinal)) { }
                if (reclaiming is not null) Require(reclaiming.CollectGarbage()!.Deleted > 0, "No retired objects were collected.");
            });
            WithStores(directory, mode, (journals, _) =>
            {
                var engine = new ClusterEngine<uint>(journals, new() { RequireDurableFlush = !OperatingSystem.IsWindows() });
                using var reader = engine.Begin("a", "b");
                Require(reader.Shard("a").GetName(1) == "winner", "Winner was not recovered.");
                Require(reader.Shard("b").GetName(1) == "revision-2 雪😀", "Atomic group was not recovered.");
                foreach (string key in new[] { "a", "b" }) Require(reader.Shard(key).GetLink(1) is not null, "Missing committed link.");
            });
            Console.WriteLine($"PASS C# native LSM {mode}: actual links, checkpoint journals, stale group rejection and file reopen");
        }
        finally { Directory.Delete(directory, true); }
    }
}
