using Foundation.Data.Doublets.Cli.Engine;

// Shared native whole-store scenario. Other runtimes use the same public API.
internal static class NativeLsmInterop
{
    private static void Require(bool condition)
    { if (!condition) throw new InvalidDataException("Native LSM interchange mismatch."); }
    private static readonly string[] Keys = ["a", "b", "c", "d"];
    private static void Check(FileLsmJournalStore store, bool advanced)
    {
        var values = store.Read(Keys); ulong[] generations = advanced ? [5, 1, 6, 3] : [2, 1, 4, 3];
        string[][] entries = [advanced ? ["handoff\0雪😀"] : ["selected\ufeff雪😀"], [], advanced ? ["revived"] : [], ["buffer"]];
        for (int i = 0; i < Keys.Length; i++) { Require(values[i].Generation == generations[i]); Require(values[i].Entries.SequenceEqual(entries[i])); }
        Require(store.ScanNamespace().Select(row => row.Key).SequenceEqual(advanced ? Keys : ["a", "b", "d"]));
        Require(!store.CompareExchange(["a"], [new(0, [])], [new[] { "lost" }]));
    }
    internal static void Run(string action, string directory, string mirrors)
    {
        if (action is not ("write" or "read" or "mutate" or "read-advanced") || mirrors is not ("plain" or "mirrored")) throw new ArgumentException("Expected write/read/mutate/read-advanced and plain/mirrored.");
        bool mirrored = mirrors == "mirrored";
        if (action == "write") Require(!Directory.EnumerateFileSystemEntries(directory).Any());
        using var store = FileLsmJournalStore.Open(directory, new(new(new LsmOptions(32, 65536, 16, 32768, 8)), 128, mirrored, mirrored, mirrored));
        if (action == "write")
        {
            Require(store.CompareExchange(["a", "b", "c"], store.Read(["a", "b", "c"]), [new[] { "old" }, Array.Empty<string>(), new[] { "removed\0雪😀" }])); store.Flush();
            Require(store.CompareExchange(["a", "b"], store.Read(["a", "b"]), [new[] { "selected\ufeff雪😀" }, null])); store.Flush();
            Require(store.CompareExchange(["d"], store.Read(["d"]), [new[] { "buffer" }])); Require(store.CompareDelete("c", 1));
        }
        Check(store, action == "read-advanced");
        if (action == "mutate")
        {
            Require(store.CompareExchange(["a", "d"], store.Read(["a", "d"]), [new[] { "handoff\0雪😀" }, null])); store.Flush(); store.Compact();
            Require(store.CompareExchange(["c"], store.Read(["c"]), [new[] { "revived" }])); Check(store, true);
        }
        Console.WriteLine($"PASS C# native LSM {action} {mirrors}");
    }
}
