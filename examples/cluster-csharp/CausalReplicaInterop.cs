using System.Globalization;
using Foundation.Data.Doublets.Cli.Engine;

internal static class CausalReplicaInterop
{
    private const string Alice = "00000000000000000000000000000001", Bob = "00000000000000000000000000000002";
    private static string Token(int n) => n.ToString("x32", CultureInfo.InvariantCulture);
    private static CausalPayload Row(string name) => new(2, 3, name);
    private static void Require(bool condition) { if (!condition) throw new InvalidDataException("Causal native interchange mismatch."); }
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
            "retry-before" or "retry-after" or "following" or "read-following") || mirrors is not ("plain" or "mirrored")) throw new ArgumentException("Invalid causal interchange action.");
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
        var owner = new CausalReplica(new CutStore(physical, action), "replica", Alice, new(RequireDurableFlush: true));
        var initial = CausalVersion.Advance(1, Alice, 0, [], Row("initial\ufeff雪😀\0"));
        var deletion = CausalVersion.Advance(1, Bob, 0, [initial], null);
        var selected = CausalVersion.Advance(1, Alice, 1, [deletion], Row("selected\ufeff雪😀\0"));
        var other = CausalVersion.Advance(2, Alice, 2, [], null);
        var advanced = CausalVersion.Advance(1, Alice, 3, [selected], null);
        var following = CausalVersion.Advance(3, Alice, 4, [], Row("following"));
        void Check(int phase)
        {
            CausalVersion[] expected = phase == 2 ? [advanced, other, following] : [phase == 1 ? advanced : selected, other];
            var snapshot = owner.Snapshot(); Require(snapshot.LastCounter == (phase == 2 ? 5UL : phase == 1 ? 4UL : 3UL));
            Require(Wires(snapshot.Versions).SequenceEqual(Wires(expected)));
            Require(owner.Write(Token(1), 1, [], initial.Payload).Encode() == initial.Encode());
            Require(!owner.Receive([initial, deletion]));
            ((CausalDot[])snapshot.Versions[0].Clock)[0] = new(Alice, 99);
            Require(Wires(owner.Read(1)).SequenceEqual(Wires([phase == 0 ? selected : advanced])));
        }
        if (action == "write")
        {
            owner.Write(Token(1), 1, [], initial.Payload); owner.Receive([deletion]);
            owner.Write(Token(2), 1, [deletion], selected.Payload); owner.Write(Token(3), 2, [], null);
            physical.Flush(); physical.Compact();
        }
        Check(action is "read-advanced" or "retry-after" or "following" ? 1 : action == "read-following" ? 2 : 0);
        if (action is "mutate" or "cut-before" or "cut-after" or "retry-before" or "retry-after")
        {
            Require(owner.Write(Token(4), 1, [selected], null).Encode() == advanced.Encode()); Check(1);
        }
        if (action == "following")
        {
            owner.Write(Token(5), 3, [], following.Payload);
            Require(owner.Write(Token(4), 1, [selected], null).Encode() == advanced.Encode()); Check(2);
        }
        if (action is "mutate" or "retry-before" or "following") { physical.Flush(); physical.Compact(); }
        Console.WriteLine($"PASS C# causal native LSM {action} {mirrors}");
    }
}
