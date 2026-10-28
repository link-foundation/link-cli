using System.Globalization;
using Foundation.Data.Doublets.Cli.Engine;

internal static class AuthorFencePhysical
{
    private const string Author = "00000000000000000000000000000001";
    private static AuthorClaim Claim(ulong epoch) => new(Author, (epoch * 10).ToString("x32", CultureInfo.InvariantCulture), epoch);
    private static readonly string[] Keys = ["a", "b"], AllKeys = ["fence", "a", "b"];
    private static string[][] Rows(ulong phase) => phase == 1 ? [["seed-a\0雪"], ["seed-b\uFEFF😀"]] :
        phase == 2 ? [["takeover-a"], ["takeover-b"]] : [["following-a"], ["following-b"]];
    private static void Require(bool value) { if (!value) throw new InvalidDataException("Author fencing interchange mismatch."); }
    private sealed class CutStore(FileLsmJournalStore physical, bool after) : IAtomicJournalStore
    {
        public bool SupportsDurableFlush => physical.SupportsDurableFlush;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys) => physical.Read(keys);
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            Require(keys.SequenceEqual(new[] { "fence" }));
            if (!after) Environment.Exit(73);
            Require(physical.CompareExchange(keys, expected, updates)); Environment.Exit(73);
            throw new InvalidOperationException("Selected takeover CAS did not exit.");
        }
    }
    private static void Fenced(Action operation)
    {
        try { operation(); }
        catch (InvalidOperationException cause) when (cause.Message.Contains("fenced", StringComparison.OrdinalIgnoreCase)) { return; }
        throw new InvalidDataException("Superseded owner admitted.");
    }
    internal static void Run(string action, string directory, string mirrors)
    {
        if (action is not ("write" or "read" or "mutate" or "read-advanced" or "blocked" or "cut-before" or "cut-after" or
            "retry-before" or "retry-after" or "read-claimed" or "following" or "read-following") || mirrors is not ("plain" or "mirrored")) throw new ArgumentException("Invalid fence interchange action.");
        bool mirrored = mirrors == "mirrored";
        var options = new FileLsmOptions(new(new LsmOptions(32, 65536, 16, 32768, 8)), 128, mirrored, mirrored, mirrored);
        if (action == "blocked")
        {
            try { using var unexpected = FileLsmJournalStore.Open(directory, options); }
            catch (IOException) { Console.WriteLine("PASS C# stale author ownership marker rejected"); return; }
            throw new InvalidDataException("Stale ownership unexpectedly admitted.");
        }
        if (action == "write") Require(!Directory.EnumerateFileSystemEntries(directory).Any());
        using var physical = FileLsmJournalStore.Open(directory, options);
        var fence = new AuthorFence(physical, "fence", Author, new(RequireDurableFlush: true));
        void Check(ulong phase, ulong dataPhase)
        {
            var state = fence.Snapshot(); Require(state.Epoch == phase && state.Claims.SequenceEqual(Enumerable.Range(1, (int)phase).Select(i => Claim((ulong)i))));
            Require(fence.Claim(phase - 1, Claim(phase).Token) == Claim(phase)); var before = physical.Read(AllKeys);
            for (ulong oldEpoch = 1; oldEpoch < phase; oldEpoch++)
            {
                Fenced(() => fence.Claim(oldEpoch - 1, Claim(oldEpoch).Token)); var stale = fence.Bind(Claim(oldEpoch));
                Fenced(() => stale.Read(Keys)); Fenced(() => stale.CompareExchange(Keys, before.Skip(1).ToArray(), Rows(1)));
            }
            var guarded = fence.Bind(Claim(phase)); var values = guarded.Read(Keys);
            Require(values.Zip(Rows(dataPhase)).All(pair => pair.First.Entries.SequenceEqual(pair.Second)));
            Require(guarded.CompareExchange(Keys, values, [null, null]));
            Require(physical.Read(AllKeys).Zip(before).All(pair => pair.First.Generation == pair.Second.Generation && pair.First.Entries.SequenceEqual(pair.Second.Entries)));
        }
        if (action == "write")
        {
            Require(fence.Claim(0, Claim(1).Token) == Claim(1)); var guarded = fence.Bind(Claim(1));
            Require(guarded.CompareExchange(Keys, [new(0, []), new(0, [])], Rows(1))); physical.Flush(); physical.Compact();
        }
        var (phase, dataPhase) = action switch { "read-advanced" => (2UL, 2UL), "retry-after" or "read-claimed" or "following" => (2UL, 1UL), "read-following" => (3UL, 3UL), _ => (1UL, 1UL) };
        Check(phase, dataPhase);
        if (action is "cut-before" or "cut-after")
        {
            new AuthorFence(new CutStore(physical, action == "cut-after"), "fence", Author, new(RequireDurableFlush: true)).Claim(1, Claim(2).Token);
            throw new InvalidOperationException("Selected takeover CAS did not exit.");
        }
        if (action is "retry-before" or "retry-after") { Require(fence.Claim(1, Claim(2).Token) == Claim(2)); Check(2, 1); }
        if (action is "mutate" or "following")
        {
            ulong next = action == "following" ? 3UL : 2UL;
            Require(fence.Claim(next - 1, Claim(next).Token) == Claim(next)); ulong generation = fence.Snapshot().Generation;
            var current = fence.Bind(Claim(next)); Require(current.CompareExchange(Keys, current.Read(Keys), Rows(next)));
            Require(fence.Snapshot().Generation == generation); Check(next, next);
        }
        if (action is "mutate" or "retry-before" or "following") { physical.Flush(); physical.Compact(); }
        Console.WriteLine($"PASS C# author fencing native LSM {action} {mirrors}");
    }
}
