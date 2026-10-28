using Foundation.Data.Doublets.Cli.Engine;

internal static class ChunkInterop
{
    private static void Require(bool value) { if (!value) throw new InvalidDataException("Chunk interchange mismatch."); }
    internal static void Run(string action, string directory, string mirrors, bool reclaiming = false)
    {
        if (action is not ("write" or "read" or "mutate" or "read-advanced") || mirrors is not ("plain" or "mirrored")) throw new ArgumentException("Invalid chunk interchange arguments.");
        if (action == "write") Require(!Directory.EnumerateFileSystemEntries(directory).Any());
        bool mirrored = mirrors == "mirrored";
        string PathFor(string name) => action == "write" ? Directory.CreateDirectory(Path.Combine(directory, name)).FullName : Path.Combine(directory, name);
        var options = new FileLsmOptions(new(new LsmOptions(256, 1048576, 64, 262144)), 128, mirrored, mirrored, mirrored);
        using var catalog = FileLsmJournalStore.Open(PathFor("catalog"), options);
        using var east = FileLsmJournalStore.Open(PathFor("east"), options);
        var chunks = new JournalChunkOptions(action == "mutate" ? 3 : 2, 64, 32);
        var collector = reclaiming ? ReclaimingPartitionedJournalStore.Open(catalog, new Dictionary<string, IJournalMaintenanceStore> { ["east"] = east }, _ => "east", new ReclamationOptions(CacheBytes: 256), chunks) : null;
        IAtomicJournalStore store = collector is not null ? collector : new PartitionedJournalStore(catalog, new Dictionary<string, IAtomicJournalStore> { ["east"] = east }, _ => "east", 256, chunks);
        string[] keys = ["a", "b"], seed = ["aa", "bb", "\ufeff雪", "😀\0", "tail"];
        if (action == "write") Require(store.CompareExchange(keys, store.Read(keys), [seed, Array.Empty<string>()]));
        void Check(bool advanced)
        {
            var values = store.Read(keys);
            Require(values[0].Generation == (reclaiming ? advanced ? 4UL : 2UL : advanced ? 2UL : 1UL) && values[1].Generation == (reclaiming ? 2UL : 1UL));
            Require(values[0].Entries.SequenceEqual(advanced ? seed.Append("next") : seed) && values[1].Entries.Count == 0);
        }
        Check(action == "read-advanced");
        if (action == "mutate")
        {
            var before = store.Read(keys);
            if (collector is not null) Require(collector.CollectGarbage()!.Epoch == 1);
            Require(store.CompareExchange(keys, before, [seed.Append("next").ToArray(), null]));
            Require(before[0].Entries.SequenceEqual(seed));
            Require(!store.CompareExchange(keys, before, [new[] { "stale" }, null])); Check(true);
            if (collector is not null) Require(collector.CollectGarbage()!.Deleted == 3);
        }
        if (action is "write" or "mutate") { catalog.Flush(); east.Flush(); catalog.Compact(); east.Compact(); }
        Console.WriteLine($"PASS C# native {(reclaiming ? "reclaiming-" : "")}chunks {action} {mirrors}");
    }
}
