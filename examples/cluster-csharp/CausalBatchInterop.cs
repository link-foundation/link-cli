using System.Globalization;
using Foundation.Data.Doublets.Cli.Engine;

internal static class CausalBatchInterop
{
    private const string Alice = "00000000000000000000000000000001";
    private static string Token(int n) => n.ToString("x32", CultureInfo.InvariantCulture);
    private static CausalPayload Row(string name) => new(2, 3, name);
    private static void Require(bool condition) { if (!condition) throw new InvalidDataException("Causal batch interchange mismatch."); }
    private static string[] Wires(IEnumerable<CausalVersion> versions) => versions.Select(v => v.Encode()).ToArray();
    private sealed class CutStore(FileLsmJournalStore physical, string action) : IAtomicJournalStore
    {
        public bool SupportsDurableFlush => physical.SupportsDurableFlush;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys) => physical.Read(keys);
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected,
            IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            if (action == "cut-before") Environment.Exit(73);
            bool result = physical.CompareExchange(keys, expected, updates);
            if (action == "cut-after" && result) Environment.Exit(73); return result;
        }
    }
    internal static void Run(string action, string directory, string mirrors)
    {
        if (action is not ("write" or "read" or "mutate" or "read-advanced" or "blocked" or "cut-before" or "cut-after" or
            "retry-before" or "retry-after" or "following" or "read-following") || mirrors is not ("plain" or "mirrored")) throw new ArgumentException("Invalid batch interchange action.");
        bool mirrored = mirrors == "mirrored";
        var options = new FileLsmOptions(new(new LsmOptions(32, 65536, 16, 32768, 8)), 128, mirrored, mirrored, mirrored);
        if (action == "blocked")
        {
            try { using var unexpected = FileLsmJournalStore.Open(directory, options); }
            catch (IOException) { Console.WriteLine("PASS C# stale ownership rejected"); return; }
            throw new InvalidDataException("Stale ownership unexpectedly admitted.");
        }
        if (action == "write") Require(!Directory.EnumerateFileSystemEntries(directory).Any());
        using var physical = FileLsmJournalStore.Open(directory, options);
        var batches = new CausalBatch(new CutStore(physical, action), "replica", "batches", Alice, new(RequireDurableFlush: true));
        CausalVersion[] first = [CausalVersion.Advance(1, Alice, 0, [], Row("initial\ufeff雪😀\0")), CausalVersion.Advance(2, Alice, 1, [], null)];
        CausalVersion[] second = [CausalVersion.Advance(1, Alice, 2, [first[0]], null), CausalVersion.Advance(2, Alice, 3, [first[1]], Row("selected\ufeff雪😀\0"))];
        var following = CausalVersion.Advance(3, Alice, 4, [], Row("following"));
        CausalBatchEdit[] InitialEdits() => first.Select(v => new CausalBatchEdit(v.Address, [], v.Payload)).ToArray();
        CausalBatchEdit[] NextEdits() => second.Select((v, i) => new CausalBatchEdit(v.Address, [first[i]], v.Payload)).ToArray();
        void Check(int phase)
        {
            CausalVersion[] expected = phase == 2 ? [.. second, following] : phase == 1 ? second : first;
            var snapshot = batches.Snapshot();
            // Empty receipts leave the physical provider's owner key unchanged.
            Require(snapshot.Generation == (phase == 2 ? 3UL : phase == 1 ? 2UL : 1UL));
            Require(snapshot.LastCounter == (phase == 2 ? 5UL : phase == 1 ? 4UL : 2UL));
            Require(Wires(snapshot.Versions).SequenceEqual(Wires(expected)));
            Require(Wires(batches.Write(Token(100), 0, InitialEdits())).SequenceEqual(Wires(first)));
            if (phase != 0) Require(Wires(batches.Write(Token(200), 1, NextEdits())).SequenceEqual(Wires(second)));
            ((CausalDot[])snapshot.Versions[0].Clock)[0] = new(Alice, 99);
            Require(Wires(batches.Snapshot().Versions).SequenceEqual(Wires(expected)));
        }
        if (action == "write")
        {
            Require(Wires(batches.Write(Token(100), 0, InitialEdits())).SequenceEqual(Wires(first)));
            physical.Flush(); physical.Compact();
        }
        Check(action is "read-advanced" or "retry-after" or "following" ? 1 : action == "read-following" ? 2 : 0);
        if (action is "mutate" or "cut-before" or "cut-after" or "retry-before" or "retry-after")
        { Require(Wires(batches.Write(Token(200), 1, NextEdits())).SequenceEqual(Wires(second))); Check(1); }
        if (action == "following")
        {
            Require(Wires(batches.Write(Token(300), 2, [new(3, [], following.Payload)])).SequenceEqual(Wires([following])));
            Require(batches.Write(Token(400), 3, []).Length == 0); Check(2);
        }
        if (action is "mutate" or "retry-before" or "following") { physical.Flush(); physical.Compact(); }
        Console.WriteLine($"PASS C# causal batch native LSM {action} {mirrors}");
    }
}
