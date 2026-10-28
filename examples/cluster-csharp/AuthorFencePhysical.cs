using System.Globalization;
using Foundation.Data.Doublets.Cli.Engine;

internal static class AuthorFencePhysical
{
    private const string Author = "00000000000000000000000000000001";
    private static AuthorClaim Claim(ulong epoch) => new(Author, (epoch * 10).ToString("x32", CultureInfo.InvariantCulture), epoch);
    private static readonly string[] Keys = ["a", "b"], AllKeys = ["fence", "a", "b"];
    private static string[][] Rows(ulong phase) => phase == 1 ? [["seed-a\0雪"], ["seed-b\uFEFF😀"]] : [["takeover-a"], ["takeover-b"]];
    private static void Require(bool value) { if (!value) throw new InvalidDataException("Author fencing interchange mismatch."); }
    private static void Fenced(Action operation)
    {
        try { operation(); }
        catch (InvalidOperationException cause) when (cause.Message.Contains("fenced", StringComparison.OrdinalIgnoreCase)) { return; }
        throw new InvalidDataException("Superseded owner admitted.");
    }
    internal static void Run(string action, string directory, string mirrors)
    {
        if (action is not ("write" or "read" or "mutate" or "read-advanced") || mirrors is not ("plain" or "mirrored")) throw new ArgumentException("Invalid fence interchange action.");
        if (action == "write") Require(!Directory.EnumerateFileSystemEntries(directory).Any());
        bool mirrored = mirrors == "mirrored";
        using var physical = FileLsmJournalStore.Open(directory, new(new(new LsmOptions(32, 65536, 16, 32768, 8)), 128, mirrored, mirrored, mirrored));
        var fence = new AuthorFence(physical, "fence", Author, new(RequireDurableFlush: true));
        void Check(ulong phase)
        {
            var state = fence.Snapshot(); Require(state.Epoch == phase && state.Claims.SequenceEqual(Enumerable.Range(1, (int)phase).Select(i => Claim((ulong)i))));
            Require(fence.Claim(phase - 1, Claim(phase).Token) == Claim(phase)); var before = physical.Read(AllKeys);
            if (phase == 2)
            {
                Fenced(() => fence.Claim(0, Claim(1).Token)); var stale = fence.Bind(Claim(1));
                Fenced(() => stale.Read(Keys)); Fenced(() => stale.CompareExchange(Keys, before.Skip(1).ToArray(), Rows(1)));
            }
            var guarded = fence.Bind(Claim(phase)); var values = guarded.Read(Keys);
            Require(values.Zip(Rows(phase)).All(pair => pair.First.Entries.SequenceEqual(pair.Second)));
            Require(guarded.CompareExchange(Keys, values, [null, null]));
            Require(physical.Read(AllKeys).Zip(before).All(pair => pair.First.Generation == pair.Second.Generation && pair.First.Entries.SequenceEqual(pair.Second.Entries)));
        }
        if (action == "write")
        {
            Require(fence.Claim(0, Claim(1).Token) == Claim(1)); var guarded = fence.Bind(Claim(1));
            Require(guarded.CompareExchange(Keys, [new(0, []), new(0, [])], Rows(1))); physical.Flush(); physical.Compact();
        }
        Check(action == "read-advanced" ? 2UL : 1UL);
        if (action == "mutate")
        {
            Require(fence.Claim(1, Claim(2).Token) == Claim(2)); ulong generation = fence.Snapshot().Generation;
            var current = fence.Bind(Claim(2)); Require(current.CompareExchange(Keys, current.Read(Keys), Rows(2)));
            Require(fence.Snapshot().Generation == generation); Check(2); physical.Flush(); physical.Compact();
        }
        Console.WriteLine($"PASS C# author fencing native LSM {action} {mirrors}");
    }
}
