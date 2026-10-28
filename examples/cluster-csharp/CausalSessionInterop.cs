using System.Globalization;
using Foundation.Data.Doublets.Cli.Engine;
using Foundation.Data.Doublets.Cli.Protocol;
using Link.Foundation.Links.Notation.Binary;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;
using Operation = Foundation.Data.Doublets.Cli.Protocol.LinksOperation<System.UInt128>;

internal static class CausalSessionInterop
{
    private const string Alice = "00000000000000000000000000000001";
    private static string Token(int n) => n.ToString("x32", CultureInfo.InvariantCulture);
    private static CausalPayload Row(string name) => new(2, 3, name);
    private static void Require(bool condition) { if (!condition) throw new InvalidDataException("Causal session interchange mismatch."); }
    private static string[] Wires(IEnumerable<CausalVersion> versions) => versions.Select(v => v.Encode()).ToArray();
    private static IReadOnlyList<LinoLink> Request(CausalSession tx, IReadOnlyList<LinoLink> document)
    {
        var reply = LinksServer.ExecuteRequest(tx.QueryLinks, document);
        if (LinksServer.ErrorMessage(reply) is { } message) throw new InvalidDataException(message);
        return reply;
    }
    private static IReadOnlyList<LinoLink> Query(CausalSession tx, string query) => Request(tx, LinksServer.QueryDocument(query));
    private static void Create(CausalSession tx, ulong id, string name)
    {
        Require(Operation.ParseChanges(Request(tx, new Operation.Create(2, 3).ToDocument())).Single().After.Index == id);
        Request(tx, new Operation.SetName(id, name).ToDocument());
    }
    private static LinoLink Change(string before, string? after)
    {
        LinoLink Row(string id) => LinoFormat.Link(id, [LinoFormat.Reference("second"), LinoFormat.Reference("3")]);
        return LinoFormat.Link(null, [LinoFormat.Link(null, [Row(before)]),
            LinoFormat.Link(null, after is null ? [] : [Row(after)])]);
    }
    private static void Prepare(CausalSession tx, int token, ulong generation, CausalBatchEdit[] expected)
    {
        var links = tx.QueryLinks; // Keep the original adapter to verify its freeze too.
        var request = tx.Prepare(Token(token));
        Require(request.Token == Token(token) && request.Generation == generation);
        string Encode(object value) => System.Text.Json.JsonSerializer.Serialize(value);
        Require(Encode(request.Edits) == Encode(expected));
        if (request.Edits.Count != 0) ((CausalBatchEdit[])request.Edits)[0] = new(99, [], null);
        Require(Encode(tx.Prepared!.Edits) == Encode(expected));
        Require(LinksServer.ErrorMessage(LinksServer.ExecuteRequest(links,
            new Operation.Count(LinksRestriction<UInt128>.All).ToDocument())) is not null);
        Require(LinksServer.ErrorMessage(LinksServer.ExecuteRequest(links,
            LinksServer.QueryDocument("(($i: $s $t)) (($i: $s $t))"))) is not null);
        Require(Encode(tx.Prepare(Token(token)).Edits) == Encode(expected));
    }
    private static string[] PhysicalBytes(string directory) => Directory.EnumerateFiles(directory).Order(StringComparer.Ordinal).Select(path =>
    {
        // The existing writer also needs write sharing from this new reader on Windows.
        using var reader = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite);
        return Path.GetFileName(path) + ":" + Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(reader));
    }).ToArray();
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
            "retry-before" or "retry-after" or "following" or "read-following") || mirrors is not ("plain" or "mirrored")) throw new ArgumentException("Invalid session interchange action.");
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
        CausalVersion[] first = [CausalVersion.Advance(1, Alice, 0, [], Row("initial\ufeff雪😀\0")), CausalVersion.Advance(2, Alice, 1, [], Row("second"))];
        CausalVersion[] second = [CausalVersion.Advance(1, Alice, 2, [first[0]], null), CausalVersion.Advance(2, Alice, 3, [first[1]], Row("selected\ufeff雪😀\0"))];
        var following = CausalVersion.Advance(3, Alice, 4, [], Row("following"));
        CausalBatchEdit[] InitialEdits() => first.Select(v => new CausalBatchEdit(v.Address, [], v.Payload)).ToArray();
        CausalBatchEdit[] NextEdits() => second.Select((v, i) => new CausalBatchEdit(v.Address, [first[i]], v.Payload)).ToArray();
        CausalSession Session(ulong first, ulong last) => new(batches, first, last, CausalConflictPolicy.Reject);
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
            using var tx = Session(1, 2);
            var bytes = PhysicalBytes(directory);
            Create(tx, 1, first[0].Payload!.Name!); Create(tx, 2, "second");
            var matched = Query(tx, "(($i: $s $t)) (($i: $s $t))");
            Require(LinoFormat.FormatDocument(matched) == LinoFormat.FormatDocument(
                [Change(first[0].Payload!.Name!, first[0].Payload!.Name!), Change("second", "second")]));
            var literal = Link.Foundation.Links.Notation.ReferenceLiteral.Encode(first[0].Payload!.Name!);
            Require(LinoFormat.FormatDocument(Query(tx, $"(({literal}: $s $t)) (({literal}: $s $t))")) ==
                LinoFormat.FormatDocument([Change(first[0].Payload!.Name!, first[0].Payload!.Name!)]));
            Require(tx.Rows().Length == 2 && batches.Snapshot().Versions.Count == 0);
            Prepare(tx, 100, 0, InitialEdits());
            Require(PhysicalBytes(directory).SequenceEqual(bytes));
            Require(Wires(tx.Commit(Token(100))).SequenceEqual(Wires(first)));
            physical.Flush(); physical.Compact();
        }
        Check(action is "read-advanced" or "retry-after" or "following" ? 1 : action == "read-following" ? 2 : 0);
        if (action is "mutate" or "cut-before" or "cut-after" or "retry-before" or "retry-after")
        {
            if (action == "retry-after")
                Require(Wires(batches.Write(Token(200), 1, NextEdits())).SequenceEqual(Wires(second)));
            else
            {
                using var stale = Session(8, 9); Request(stale, new Operation.SetName(1, "stale").ToDocument());
                using var tx = Session(3, 3);
                var privateBytes = PhysicalBytes(directory);
                var observed = Query(tx, "(($i: $s $t)) (($i: $s $t))");
                var originalReply = LinoFormat.FormatDocument(observed);
                var deleted = Query(tx, "((1: $s $t)) ()");
                Require(LinoFormat.FormatDocument(deleted) == LinoFormat.FormatDocument([Change("1", null)]));
                Request(tx, new Operation.SetName(2, second[1].Payload!.Name!).ToDocument());
                Require(tx.GetLink(1) is null && tx.GetName(2) == second[1].Payload!.Name);
                Require(LinoFormat.FormatDocument(observed) == originalReply);
                Prepare(tx, 200, 1, NextEdits());
                Require(PhysicalBytes(directory).SequenceEqual(privateBytes));
                Require(Wires(tx.Commit(Token(200))).SequenceEqual(Wires(second)));
                string[] keys = ["replica", "batches"];
                string before = System.Text.Json.JsonSerializer.Serialize(physical.Read(keys));
                string[] bytes = PhysicalBytes(directory);
                try { stale.Commit(Token(500)); throw new InvalidDataException("Stale session admitted."); }
                catch (InvalidOperationException) { }
                Require(System.Text.Json.JsonSerializer.Serialize(physical.Read(keys)) == before);
                Require(PhysicalBytes(directory).SequenceEqual(bytes));
            }
            Check(1);
        }
        if (action == "following")
        {
            using var tx = Session(3, 3); var bytes = PhysicalBytes(directory);
            Create(tx, 3, "following"); Prepare(tx, 300, 2, [new(3, [], following.Payload)]);
            Require(PhysicalBytes(directory).SequenceEqual(bytes));
            Require(Wires(tx.Commit(Token(300))).SequenceEqual(Wires([following])));
            using var empty = Session(4, 4); Require(empty.Commit(Token(400)).Length == 0); Check(2);
        }
        if (action is "mutate" or "retry-before" or "following") { physical.Flush(); physical.Compact(); }
        Console.WriteLine($"PASS C# causal session native LSM {action} {mirrors}");
    }
}
