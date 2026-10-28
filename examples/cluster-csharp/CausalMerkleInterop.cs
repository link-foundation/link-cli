using System.Globalization;
using System.Text;
using Foundation.Data.Doublets.Cli.Engine;

internal static class CausalMerkleInterop
{
    private const string Alice = "00000000000000000000000000000001", Bob = "00000000000000000000000000000002";
    private const string Local = "0000000000000000000000000000000d";
    private static string Token(int n) => n.ToString("x32", CultureInfo.InvariantCulture);
    private static CausalPayload Row(string name) => new(2, 3, name);
    private static void Require(bool value) { if (!value) throw new InvalidDataException("Merkle interchange mismatch."); }
    private static string[] Wires(IEnumerable<CausalVersion> values) => values.Select(v => v.Encode()).Order(StringComparer.Ordinal).ToArray();
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
    private sealed class Source(string root) : ICausalMerkleSource
    {
        public string Root => root;
        internal readonly Dictionary<string, string> Nodes = new(StringComparer.Ordinal);
        internal readonly Dictionary<ulong, string> Leaves = [];
        public CausalMerkleNode Node(string prefix) => CausalMerkleSnapshot.DecodeNode(root, Nodes[prefix]);
        public CausalMerkleLeaf Leaf(ulong address) => CausalMerkleSnapshot.DecodeLeaf(root, Leaves[address]);
    }
    internal static void Run(string action, string directory, string mirrors, string messageFile)
    {
        if (action is not ("export" or "seed" or "pull" or "read-pulled" or "following" or "read-following" or "cut-before" or "cut-after") ||
            mirrors is not ("plain" or "mirrored")) throw new ArgumentException("Invalid Merkle interchange action.");
        bool mirrored = mirrors == "mirrored";
        using var physical = FileLsmJournalStore.Open(directory, new(new(new LsmOptions(32, 65536, 16, 32768, 8)), 128, mirrored, mirrored, mirrored));
        var owner = new CausalReplica(new CutStore(physical, action), "replica", action == "export" ? Alice : Local, new(RequireDurableFlush: true));
        var initial = CausalVersion.Advance(1, Alice, 0, [], Row("initial\ufeff雪😀\0"));
        var deletion = CausalVersion.Advance(1, Bob, 0, [initial], null);
        var selected = CausalVersion.Advance(1, Alice, 1, [deletion], Row("selected\ufeff雪😀\0"));
        var other = CausalVersion.Advance(2, Alice, 2, [], null);
        var concurrent = CausalVersion.Advance(1, Local, 0, [], Row("concurrent"));
        var resolved = CausalVersion.Advance(1, Local, 1, [selected, concurrent], null);
        void Check(bool following)
        {
            var state = owner.Snapshot(); Require(state.LastCounter == (following ? 2UL : 1UL));
            Require(Wires(state.Versions).SequenceEqual(Wires(following ? [resolved, other] : [selected, concurrent, other])));
            Require(owner.Write(Token(1), 1, [], Row("concurrent")).Encode() == concurrent.Encode());
        }
        if (action == "export")
        {
            var snapshot = CausalMerkleSnapshot.Create(owner.Snapshot().Versions); var lines = new List<string> { "causal-bundle1", snapshot.Root };
            var prefixes = new SortedSet<string>(StringComparer.Ordinal);
            foreach (var value in new[] { selected, other })
            {
                string path = value.Address.ToString("x16", CultureInfo.InvariantCulture);
                for (int depth = 0; depth < 16; depth++) prefixes.Add(path[..depth]);
            }
            foreach (string prefix in prefixes) lines.Add(CausalMerkleSnapshot.EncodeNode(snapshot.Node(prefix)));
            foreach (var value in new[] { selected, other }) lines.Add(CausalMerkleSnapshot.EncodeLeaf(snapshot.Leaf(value.Address)));
            string wire = string.Join('\n', lines) + "\n"; Require(wire.Length <= 524288); File.WriteAllText(messageFile, wire, new UTF8Encoding(false));
        }
        else if (action == "seed") Require(owner.Write(Token(1), 1, [], Row("concurrent")).Encode() == concurrent.Encode());
        else if (action is "read-pulled" or "read-following") Check(action == "read-following");
        else if (action == "following")
        {
            Check(false); Require(owner.Write(Token(2), 1, owner.Read(1), null).Encode() == resolved.Encode()); Check(true);
        }
        else
        {
            byte[] buffer = new byte[524289]; int size = 0;
            using (var stream = File.OpenRead(messageFile))
            {
                while (size < buffer.Length) { int count = stream.Read(buffer.AsSpan(size)); if (count == 0) break; size += count; }
            }
            Require(size <= 524288); string[] lines = Encoding.UTF8.GetString(buffer, 0, size).Split('\n');
            Require(lines.Length is >= 4 and <= 43 && lines[^1] == "" && lines[0] == "causal-bundle1"); var source = new Source(lines[1]);
            foreach (string wire in lines[2..^1])
            {
                if (wire.StartsWith("[\"causal-node1\",", StringComparison.Ordinal))
                {
                    var node = CausalMerkleSnapshot.DecodeNode(source.Root, wire); source.Nodes.Add(node.Prefix, wire);
                }
                else
                {
                    var leaf = CausalMerkleSnapshot.DecodeLeaf(source.Root, wire); source.Leaves.Add(leaf.Address, wire);
                }
            }
            CausalPullCursor? continuation = null; bool complete = false;
            for (int attempt = 0; attempt < 40; attempt++)
            {
                var progress = CausalMerkleTransfer.Pull(source, owner, new(MaxNodeRequests: 4, MaxLeafRequests: 1, Continuation: continuation));
                Require(progress.NodeRequests <= 4 && progress.LeafRequests <= 1);
                if (progress.Complete) { complete = true; break; }
                continuation = progress.Continuation;
            }
            Require(complete); Check(false);
            var retry = CausalMerkleTransfer.Pull(source, owner); Require(retry.Complete && retry.ChangedLeaves == 0);
        }
        Console.WriteLine($"PASS C# Merkle native LSM {action} {mirrors}");
    }
}
