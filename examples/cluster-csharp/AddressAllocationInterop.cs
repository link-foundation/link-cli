using System.Globalization;
using Foundation.Data.Doublets.Cli.Engine;

internal static class AddressAllocationInterop
{
    private const string Alice = "00000000000000000000000000000001", Bob = "00000000000000000000000000000002";
    private static string Token(int value) => value.ToString("x32", CultureInfo.InvariantCulture);
    private static void Require(bool condition) { if (!condition) throw new InvalidDataException("Address allocation interchange mismatch."); }
    private sealed class CutStore(FileLsmJournalStore physical, string action) : IAtomicJournalStore
    {
        public bool SupportsDurableFlush => physical.SupportsDurableFlush;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys) => physical.Read(keys);
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            if (action == "cut-before") Environment.Exit(73);
            bool result = physical.CompareExchange(keys, expected, updates);
            if (action == "cut-after" && result) Environment.Exit(73); return result;
        }
    }
    internal static void Run(string action, string directory, string mirrors)
    {
        if (action is not ("write" or "read" or "mutate" or "read-advanced" or "blocked" or "cut-before" or "cut-after" or
            "retry-before" or "retry-after" or "following" or "read-following") || mirrors is not ("plain" or "mirrored")) throw new ArgumentException("Invalid allocation interchange action.");
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
        var owner = new AddressAllocator(new CutStore(physical, action), "addresses", 1, 20, new(RequireDurableFlush: true));
        AddressReservation[] grants = [new(Alice, Token(100), 2, 1, 2), new(Bob, Token(200), 3, 3, 5), new(Alice, Token(300), 2, 6, 7)];
        void Reserve(int index) { var grant = grants[index]; Require(owner.Reserve(grant.Author, grant.Token, grant.Count) == grant); }
        void Check(int phase)
        {
            Require(owner.Snapshot().Reservations.SequenceEqual(grants.Take(phase + 1)));
            Require(owner.Snapshot().Next == new ulong[] { 3, 6, 8 }[phase]);
            for (int i = 0; i <= phase; i++) Reserve(i);
            foreach (var request in new[] { (Bob, Token(100), 2UL), (Alice, Token(100), 1UL) })
            {
                try { owner.Reserve(request.Item1, request.Item2, request.Item3); throw new InvalidOperationException("Aliased token admitted."); }
                catch (InvalidDataException) { }
            }
            try { new AddressAllocator(physical, "addresses", 2, 20).Snapshot(); throw new InvalidOperationException("Changed domain admitted."); }
            catch (InvalidDataException) { }
        }
        if (action == "write") { Reserve(0); physical.Flush(); physical.Compact(); }
        Check(action is "read-advanced" or "retry-after" or "following" ? 1 : action == "read-following" ? 2 : 0);
        if (action is "mutate" or "cut-before" or "cut-after" or "retry-before" or "retry-after") { Reserve(1); Check(1); }
        if (action == "following") { Reserve(2); Check(2); }
        if (action is "mutate" or "retry-before" or "following") { physical.Flush(); physical.Compact(); }
        Console.WriteLine($"PASS C# address allocation native LSM {action} {mirrors}");
    }
    internal static void Etcd(string endpoints, string prefix, string author, string token, string count)
    {
        using var store = new EtcdJournalStore(endpoints.Split(',').Select(value => new Uri(value)), prefix);
        var grant = new AddressAllocator(store, "addresses", 1, ulong.MaxValue, new(RequireDurableFlush: true))
            .Reserve(author, token, ulong.Parse(count, NumberStyles.None, CultureInfo.InvariantCulture));
        Console.WriteLine(System.Text.Json.JsonSerializer.Serialize(new { author = grant.Author, token = grant.Token,
            count = grant.Count.ToString(CultureInfo.InvariantCulture), first = grant.First.ToString(CultureInfo.InvariantCulture), last = grant.Last.ToString(CultureInfo.InvariantCulture) }));
    }
}
