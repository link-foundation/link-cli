using System.Globalization;
using System.Text.Json;
using System.Text.Json.Nodes;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class CausalMerkleTests
{
    private static JsonElement Fixtures()
    {
        using var document = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "causal-merkle-v1.json")));
        return document.RootElement.Clone();
    }
    private static CausalVersion[] Values(JsonElement items) => items.EnumerateArray().Select(item => CausalVersion.Decode(JsonSerializer.Serialize(item))).ToArray();
    private static string[][] Proof(JsonElement items) => items.EnumerateArray().Select(level => level.EnumerateArray().Select(item => item.GetString()!).ToArray()).ToArray();
    private static string[] Wires(IEnumerable<CausalVersion> values) => values.Select(v => v.Encode()).Order(StringComparer.Ordinal).ToArray();
    private static string Token(int value) => value.ToString("x32", CultureInfo.InvariantCulture);
    private static CausalReplica Owner(Store store) => new(store, "replica", "0000000000000000000000000000000e");
    private sealed class Store : IAtomicJournalStore
    {
        internal ulong Generation; internal string[] Entries = []; internal int Calls, Fault;
        public bool SupportsDurableFlush => true;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys) => [new(Generation, Entries.ToArray())];
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            Calls++; int fault = Fault; Fault = 0;
            if (fault == 1) throw new IOException("Before publication.");
            if (expected[0].Generation != Generation) return false;
            Generation++; Entries = updates[0]!.ToArray(); if (fault == 2) throw new IOException("Lost reply."); return true;
        }
    }
    private sealed class Remote(CausalMerkleSnapshot snapshot) : ICausalMerkleSource
    {
        internal Func<string, CausalMerkleNode>? OnNode;
        internal Func<ulong, CausalMerkleLeaf>? OnLeaf;
        public string Root => snapshot.Root;
        public CausalMerkleNode Node(string prefix) => OnNode is null ? snapshot.Node(prefix) : OnNode(prefix);
        public CausalMerkleLeaf Leaf(ulong address) => OnLeaf is null ? snapshot.Leaf(address) : OnLeaf(address);
    }

    [Fact]
    public void IndependentCanonicalRootsAndProofsMatch()
    {
        var fixtures = Fixtures(); var snapshots = fixtures.GetProperty("snapshots").EnumerateArray().Select(item =>
        {
            var snapshot = CausalMerkleSnapshot.Create(Values(item.GetProperty("versions")));
            Assert.Equal(item.GetProperty("root").GetString(), snapshot.Root);
            Assert.Equal(16, CausalMerkleSnapshot.VerifyNode(snapshot.Root, snapshot.Node("")).Length); return snapshot;
        }).ToArray();
        foreach (var item in fixtures.GetProperty("leaves").EnumerateArray())
        {
            var expected = new CausalMerkleLeaf(item.GetProperty("root").GetString()!, ulong.Parse(item.GetProperty("address").GetString()!, CultureInfo.InvariantCulture),
                Values(item.GetProperty("versions")), Proof(item.GetProperty("proof")));
            var actual = snapshots[item.GetProperty("snapshot").GetInt32()].Leaf(expected.Address);
            Assert.Equal(JsonSerializer.Serialize(expected), JsonSerializer.Serialize(actual));
            Assert.Equal(Wires(expected.Versions), Wires(CausalMerkleSnapshot.VerifyLeaf(expected.Root, expected)));
            Assert.Equal(item.GetProperty("wire").GetString(), CausalMerkleSnapshot.EncodeLeaf(expected));
            Assert.Equal(JsonSerializer.Serialize(expected), JsonSerializer.Serialize(CausalMerkleSnapshot.DecodeLeaf(expected.Root, item.GetProperty("wire").GetString()!)));
        }
        foreach (var item in fixtures.GetProperty("nodes").EnumerateArray())
        {
            var expected = new CausalMerkleNode(item.GetProperty("root").GetString()!, item.GetProperty("prefix").GetString()!,
                item.GetProperty("children").EnumerateArray().Select(v => v.GetString()!).ToArray(), Proof(item.GetProperty("proof")));
            var actual = snapshots[item.GetProperty("snapshot").GetInt32()].Node(expected.Prefix);
            Assert.Equal(JsonSerializer.Serialize(expected), JsonSerializer.Serialize(actual));
            Assert.Equal(expected.Children, CausalMerkleSnapshot.VerifyNode(expected.Root, expected));
            Assert.Equal(item.GetProperty("wire").GetString(), CausalMerkleSnapshot.EncodeNode(expected));
            Assert.Equal(JsonSerializer.Serialize(expected), JsonSerializer.Serialize(CausalMerkleSnapshot.DecodeNode(expected.Root, item.GetProperty("wire").GetString()!)));
        }
    }

    [Fact]
    public void SnapshotAndMessagesOwnTheirNestedState()
    {
        var fixture = Fixtures().GetProperty("snapshots")[3]; var input = Values(fixture.GetProperty("versions"));
        var snapshot = CausalMerkleSnapshot.Create(input); ((CausalDot[])input[0].Clock)[0] = new(input[0].Dot.Author, 99);
        input[0] = input[0] with { Payload = null };
        Assert.Equal(fixture.GetProperty("root").GetString(), snapshot.Root);
        var leaf = snapshot.Leaf(1); ((CausalDot[])leaf.Versions[0].Clock)[0] = new(leaf.Versions[0].Dot.Author, 99);
        ((string[][])leaf.Proof)[0][0] = new('0', 64);
        var node = snapshot.Node(""); ((string[])node.Children)[0] = new('0', 64);
        Assert.Equal(Wires(Values(fixture.GetProperty("versions"))), Wires(CausalMerkleSnapshot.VerifyLeaf(snapshot.Root, snapshot.Leaf(1))));
        CausalMerkleSnapshot.VerifyNode(snapshot.Root, snapshot.Node(""));
    }

    [Fact]
    public void MalformedMixedAndChangedProofsReject()
    {
        var fixtures = Fixtures(); var snapshot = CausalMerkleSnapshot.Create(Values(fixtures.GetProperty("snapshots")[3].GetProperty("versions")));
        var leaf = snapshot.Leaf(1); var changed = leaf.Versions.ToArray(); changed[0] = changed[0] with { Payload = new(1, 1, "forged") };
        string[][] badProof = leaf.Proof.Select(v => v.ToArray()).ToArray(); badProof[0][0] = new('0', 64);
        foreach (var bad in new[] { leaf with { Root = new('0', 64) }, leaf with { Address = 0 }, leaf with { Address = 2 },
            leaf with { Versions = leaf.Versions.Reverse().ToArray() }, leaf with { Versions = [.. leaf.Versions, leaf.Versions[0]] },
            leaf with { Versions = changed }, leaf with { Proof = leaf.Proof.Skip(1).ToArray() }, leaf with { Proof = badProof },
            leaf with { Proof = null! }, leaf with { Proof = [null!, .. leaf.Proof.Skip(1)] },
            leaf with { Proof = [Array.Empty<string>(), .. leaf.Proof.Skip(1)] }, leaf with { Versions = null! } })
            Assert.Throws<InvalidDataException>(() => CausalMerkleSnapshot.VerifyLeaf(snapshot.Root, bad));
        var node = snapshot.Node("000000000000000"); var children = node.Children.ToArray(); children[0] = new('0', 64);
        foreach (var bad in new[] { node with { Root = new('0', 64) }, node with { Prefix = "F" }, node with { Prefix = new('0', 16) },
            node with { Prefix = "1" + node.Prefix[1..] }, node with { Children = node.Children.Skip(1).ToArray() },
            node with { Children = children }, node with { Proof = node.Proof.Skip(1).ToArray() }, node with { Children = null! } })
            Assert.Throws<InvalidDataException>(() => CausalMerkleSnapshot.VerifyNode(snapshot.Root, bad));
        Assert.Throws<InvalidDataException>(() => CausalMerkleSnapshot.VerifyNode("bad", node));
        Assert.Throws<InvalidDataException>(() => CausalMerkleSnapshot.VerifyLeaf("bad", leaf));
        Assert.Throws<InvalidDataException>(() => CausalMerkleSnapshot.VerifyNode(snapshot.Root, null!));
        Assert.Throws<InvalidDataException>(() => CausalMerkleSnapshot.VerifyLeaf(snapshot.Root, null!));
        foreach (string wire in new[] { null!, "", "null", "[]", "{}", "}", "[\"other\"]", "[", "]", "\"", "[\"\\\\\"]", "[\"雪\"]", "[\"\n\"]", new string('[', 6) + new string(']', 6), "[" + new string(',', 32769) + "]", new string('x', 1_081_345) })
        {
            Assert.NotNull(Record.Exception(() => CausalMerkleSnapshot.DecodeLeaf(snapshot.Root, wire)));
            Assert.NotNull(Record.Exception(() => CausalMerkleSnapshot.DecodeNode(snapshot.Root, wire)));
        }
        foreach (var item in fixtures.GetProperty("leaves").EnumerateArray())
        {
            string wire = item.GetProperty("wire").GetString()!, root = item.GetProperty("root").GetString()!;
            foreach (string bad in new[] { wire + " ", wire.Replace("causal-leaf1", "causal-leaf2", StringComparison.Ordinal), wire.Replace("causal-leaf1", "causal-\\u006ceaf1", StringComparison.Ordinal) })
                Assert.NotNull(Record.Exception(() => CausalMerkleSnapshot.DecodeLeaf(root, bad)));
            foreach (var (index, replacement) in new (int, JsonNode?)[] { (1, null), (2, JsonValue.Create(1)), (2, JsonValue.Create("")), (2, JsonValue.Create(new string('1', 21))), (2, JsonValue.Create("+1")), (2, JsonValue.Create("-1")), (2, JsonValue.Create(" 1")), (2, JsonValue.Create("1 ")), (2, JsonValue.Create("invalid")), (2, JsonValue.Create("01")), (2, JsonValue.Create("0")), (2, JsonValue.Create("18446744073709551616")), (3, null), (4, null) })
            {
                var array = JsonNode.Parse(wire)!.AsArray(); array[index] = replacement?.DeepClone();
                Assert.NotNull(Record.Exception(() => CausalMerkleSnapshot.DecodeLeaf(root, array.ToJsonString(new() { WriteIndented = false }))));
            }
        }
        string nodeWire = fixtures.GetProperty("nodes")[1].GetProperty("wire").GetString()!;
        foreach (int index in new[] { 1, 2, 3, 4 })
        {
            var array = JsonNode.Parse(nodeWire)!.AsArray(); array[index] = null;
            Assert.NotNull(Record.Exception(() => CausalMerkleSnapshot.DecodeNode(node.Root, array.ToJsonString(new() { WriteIndented = false }))));
        }
    }

    [Fact]
    public void AdmissionRetainsDeletionsAndRejectsContradictionsBeforePruning()
    {
        var original = Values(Fixtures().GetProperty("snapshots")[1].GetProperty("versions"))[0];
        var newer = CausalVersion.Advance(1, original.Dot.Author, 1, [original], null);
        var snapshot = CausalMerkleSnapshot.Create([original, newer, original]); Assert.Equal(newer.Encode(), snapshot.Leaf(1).Versions[0].Encode());
        CausalMerkleSnapshot.VerifyLeaf(snapshot.Root, snapshot.Leaf(1));
        Assert.Throws<InvalidDataException>(() => CausalMerkleSnapshot.Create([original, original with { Payload = new(1, 1, "different") }]));
        Assert.Throws<InvalidDataException>(() => CausalMerkleSnapshot.Create([original, original with { Address = 2 }]));
        foreach (var options in new[] { new CausalMerkleOptions(0), new(4097), new(1, 0), new(1, 8_388_609) })
            Assert.Throws<InvalidDataException>(() => CausalMerkleSnapshot.Create([], options));
        Assert.Throws<InvalidDataException>(() => CausalMerkleSnapshot.Create([original, original], new(1)));
        Assert.Throws<InvalidDataException>(() => CausalMerkleSnapshot.Create([original], new(MaxBytes: 1)));
        Assert.Throws<InvalidDataException>(() => CausalMerkleSnapshot.Create(Enumerable.Repeat(original, 65).ToArray()));
        Assert.Throws<InvalidDataException>(() => CausalMerkleSnapshot.Create(null!));
        Assert.Throws<InvalidDataException>(() => snapshot.Leaf(0)); Assert.Throws<InvalidDataException>(() => snapshot.Node("x"));
        var response = snapshot.Leaf(1); Assert.Throws<InvalidDataException>(() => CausalMerkleSnapshot.VerifyLeaf(snapshot.Root, response with { Versions = [original, newer] }));
    }

    [Fact]
    public void AllSnapshotPairsMatchDirectCausalUnionAndIdempotentRetry()
    {
        var cases = Fixtures().GetProperty("snapshots").EnumerateArray().ToArray();
        foreach (var sourceCase in cases) foreach (var destinationCase in cases)
        {
            var source = CausalMerkleSnapshot.Create(Values(sourceCase.GetProperty("versions"))); var store = new Store(); var destination = Owner(store);
            destination.Receive(Values(destinationCase.GetProperty("versions"))); var progress = CausalMerkleTransfer.Pull(source, destination);
            Assert.Equal(source.Root, progress.Root); Assert.True(progress.Complete);
            var expected = Values(sourceCase.GetProperty("versions")).Concat(Values(destinationCase.GetProperty("versions"))).GroupBy(v => v.Address)
                .SelectMany(group => CausalVersion.Merge(group.ToArray(), [])); Assert.Equal(Wires(expected), Wires(destination.Snapshot().Versions));
            int calls = store.Calls; var retry = CausalMerkleTransfer.Pull(source, destination);
            Assert.True(retry.Complete); Assert.Equal(0, retry.ChangedLeaves); Assert.Equal(calls, store.Calls);
            if (sourceCase.GetProperty("root").GetString() == destinationCase.GetProperty("root").GetString() || sourceCase.GetProperty("versions").GetArrayLength() == 0)
            { Assert.Equal(0, progress.NodeRequests); Assert.Equal(0, progress.LeafRequests); }
        }
    }

    [Fact]
    public void BoundedAttemptsAndInterruptedTransportResumeFromFreshState()
    {
        var source = CausalMerkleSnapshot.Create(Values(Fixtures().GetProperty("snapshots")[5].GetProperty("versions")));
        var store = new Store(); var destination = Owner(store);
        var shortAttempt = CausalMerkleTransfer.Pull(source, destination, new(MaxNodeRequests: 1));
        Assert.False(shortAttempt.Complete); Assert.Equal(1, shortAttempt.NodeRequests); Assert.Equal(0, shortAttempt.LeafRequests); Assert.Equal(0, store.Calls);
        Assert.False(CausalMerkleTransfer.Pull(source, destination, new(MaxVersionBytes: 1)).Complete); Assert.Equal(0, store.Calls);
        var first = CausalMerkleTransfer.Pull(source, destination, new(MaxLeafRequests: 1)); Assert.False(first.Complete); Assert.Equal(1, first.ChangedLeaves);
        int reads = 0; var remote = new Remote(source) { OnLeaf = address => ++reads == 2 ? throw new IOException("Interrupted.") : source.Leaf(address) };
        Assert.Throws<IOException>(() => CausalMerkleTransfer.Pull(remote, destination)); Assert.Equal(2, destination.Snapshot().Versions.Count);
        Assert.True(CausalMerkleTransfer.Pull(source, destination).Complete); Assert.Equal(Wires(Values(Fixtures().GetProperty("snapshots")[5].GetProperty("versions"))), Wires(destination.Snapshot().Versions));
        foreach (var options in new[] { new CausalPullOptions(0), new(65537), new(MaxLeafRequests: 0), new(MaxLeafRequests: 4097), new(MaxVersionBytes: 0), new(MaxVersionBytes: 8_388_609) })
            Assert.Throws<InvalidDataException>(() => CausalMerkleTransfer.Pull(source, destination, options));
        Assert.Throws<InvalidDataException>(() => CausalMerkleTransfer.Pull(null!, destination));
        Assert.Throws<InvalidDataException>(() => CausalMerkleTransfer.Pull(source, null!));
    }

    [Fact]
    public void ConcurrentWritesAndLostRepliesRetainCausalUnion()
    {
        var source = CausalMerkleSnapshot.Create(Values(Fixtures().GetProperty("snapshots")[1].GetProperty("versions")));
        var store = new Store(); var destination = Owner(store); bool changed = false;
        var remote = new Remote(source)
        {
            OnNode = prefix =>
        { if (!changed) { changed = true; destination.Write(Token(1), 1, [], new(0, 0, "concurrent")); } return source.Node(prefix); }
        };
        CausalMerkleTransfer.Pull(remote, destination); Assert.Equal(2, destination.Read(1).Length);
        Assert.Contains(destination.Read(1), v => v.Payload?.Name == "concurrent");
        foreach (int fault in new[] { 1, 2 })
        {
            var failed = new Store { Fault = fault }; Assert.Throws<IOException>(() => CausalMerkleTransfer.Pull(source, Owner(failed)));
            int calls = failed.Calls; var reopened = Owner(failed); Assert.True(CausalMerkleTransfer.Pull(source, reopened).Complete);
            Assert.Equal(calls + (fault == 1 ? 1 : 0), failed.Calls); reopened.Write(Token(1), 2, [], null); Assert.Equal(1UL, reopened.Snapshot().LastCounter);
        }
    }

    [Fact]
    public void RequestedRootAndPathAreVerifiedBeforePublication()
    {
        var source = CausalMerkleSnapshot.Create(Values(Fixtures().GetProperty("snapshots")[1].GetProperty("versions")));
        var other = CausalMerkleSnapshot.Create(Values(Fixtures().GetProperty("snapshots")[4].GetProperty("versions")));
        foreach (var remote in new[] { new Remote(source) { OnNode = _ => null! }, new(source) { OnLeaf = _ => null! },
            new(source) { OnNode = _ => other.Node("") }, new(source) { OnNode = _ => source.Node("0") },
            new(source) { OnLeaf = _ => source.Leaf(2) }, new(source) { OnLeaf = other.Leaf }, new(source) { OnLeaf = address =>
            { var leaf = source.Leaf(address); return leaf with { Versions = [leaf.Versions[0] with { Payload = new(1, 1, "forged") }] }; } } })
        {
            var store = new Store(); Assert.Throws<InvalidDataException>(() => CausalMerkleTransfer.Pull(remote, Owner(store))); Assert.Equal(0, store.Calls);
        }
    }

    [Fact]
    public void BoundedContinuationReachesEveryAddressWithConcurrentFirstLeaf()
    {
        var source = CausalMerkleSnapshot.Create(Values(Fixtures().GetProperty("snapshots")[5].GetProperty("versions")));
        var destination = Owner(new Store()); var concurrent = destination.Write(Token(1), 1, [], new(0, 0, "local"));
        var progress = CausalMerkleTransfer.Pull(source, destination, new(MaxNodeRequests: 1));
        var cursor = Assert.IsType<CausalPullCursor>(progress.Continuation); Assert.Equal(source.Root, cursor.Root);
        Assert.Throws<InvalidDataException>(() => cursor.Resume(new string('0', 64), destination.Snapshot().Author));
        var frontier = cursor.Resume(source.Root, destination.Snapshot().Author);
        string firstPath = frontier[0]; frontier[0] = "changed copy";
        Assert.Equal(firstPath, cursor.Resume(source.Root, destination.Snapshot().Author)[0]);
        var other = CausalMerkleSnapshot.Create([]);
        Assert.Throws<InvalidDataException>(() => CausalMerkleTransfer.Pull(other, destination, new(Continuation: cursor)));
        var foreignStore = new Store(); var foreign = new CausalReplica(foreignStore, "replica", "0000000000000000000000000000000f");
        Assert.Throws<InvalidDataException>(() => CausalMerkleTransfer.Pull(source, foreign, new(Continuation: cursor))); Assert.Equal(0, foreignStore.Calls);
        for (int attempt = 0; attempt < 128; attempt++)
        {
            progress = CausalMerkleTransfer.Pull(source, destination, new(MaxNodeRequests: 1, MaxLeafRequests: 1, Continuation: progress.Continuation));
            Assert.InRange(progress.NodeRequests, 0, 1); Assert.InRange(progress.LeafRequests, 0, 1); if (progress.Complete) break;
        }
        Assert.True(progress.Complete); Assert.Null(progress.Continuation);
        Assert.Equal(Wires(Values(Fixtures().GetProperty("snapshots")[5].GetProperty("versions")).Append(concurrent)), Wires(destination.Snapshot().Versions));
        Assert.True(CausalMerkleTransfer.Pull(source, destination, new(Continuation: cursor)).Complete);
        var identical = Owner(new Store()); identical.Receive(Values(Fixtures().GetProperty("snapshots")[5].GetProperty("versions")));
        Assert.Null(CausalMerkleTransfer.Pull(source, identical, new(Continuation: cursor)).Continuation);
        var leafLimited = Owner(new Store()); leafLimited.Write(Token(1), 1, [], new(0, 0, "local"));
        CausalPullCursor? leafCursor = null; bool complete = false;
        for (int attempt = 0; attempt < 8; attempt++)
        {
            var next = CausalMerkleTransfer.Pull(source, leafLimited, new(MaxLeafRequests: 1, Continuation: leafCursor));
            Assert.InRange(next.LeafRequests, 0, 1); if (next.Complete) { complete = true; break; }
            leafCursor = next.Continuation;
        }
        Assert.True(complete);
        Assert.Equal(Wires(Values(Fixtures().GetProperty("snapshots")[5].GetProperty("versions")).Append(concurrent)), Wires(leafLimited.Snapshot().Versions));
    }
}
