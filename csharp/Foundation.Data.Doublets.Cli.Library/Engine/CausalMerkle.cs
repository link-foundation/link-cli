using System.Buffers.Binary;
using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Engine;

public sealed record CausalMerkleOptions(int MaxVersions = 4096, int MaxBytes = 8_388_608);
public sealed record CausalMerkleNode(string Root, string Prefix, IReadOnlyList<string> Children,
    IReadOnlyList<IReadOnlyList<string>> Proof);
public sealed record CausalMerkleLeaf(string Root, ulong Address, IReadOnlyList<CausalVersion> Versions,
    IReadOnlyList<IReadOnlyList<string>> Proof);
public interface ICausalMerkleSource
{
    string Root { get; }
    CausalMerkleNode Node(string prefix);
    CausalMerkleLeaf Leaf(ulong address);
}

/// <summary>Selected immutable visible-content index. No store handle or global
/// snapshot registry is retained; the caller bounds its snapshot lifetime.</summary>
public sealed class CausalMerkleSnapshot : ICausalMerkleSource
{
    private static readonly byte[] Domain = Encoding.ASCII.GetBytes("causal-merkle1\0");
    private static readonly byte[][] Empty = EmptyHashes();
    private readonly Dictionary<ulong, CausalVersion[]> _rows = [];
    private readonly Dictionary<string, byte[]> _nodes = new(StringComparer.Ordinal);
    public string Root { get; }
    private static InvalidDataException Invalid() => new("Invalid causal Merkle state.");
    private static CausalVersion Owned(CausalVersion value) => CausalVersion.Decode(value.Encode());
    private static string Hex(byte[] value) => Convert.ToHexStringLower(value);
    private static byte[] Hash(string value)
    {
        if (value is null || value.Length != 64 || value.Any(c => !char.IsAsciiDigit(c) && c is not (>= 'a' and <= 'f'))) throw Invalid();
        return Convert.FromHexString(value);
    }
    private static void Prefix(string value)
    {
        if (value is null || value.Length > 15 || value.Any(c => !char.IsAsciiDigit(c) && c is not (>= 'a' and <= 'f'))) throw Invalid();
    }
    private static byte[] Branch(int depth, IReadOnlyList<byte[]> children)
    {
        using var bytes = new MemoryStream(); bytes.Write(Domain); bytes.WriteByte((byte)'N'); bytes.WriteByte((byte)depth);
        foreach (var child in children) bytes.Write(child); return SHA256.HashData(bytes.GetBuffer().AsSpan(0, (int)bytes.Length));
    }
    private static byte[][] EmptyHashes()
    {
        var result = new byte[17][]; result[16] = SHA256.HashData([.. Domain, (byte)'E']);
        for (int depth = 15; depth >= 0; depth--) result[depth] = Branch(depth, Enumerable.Repeat(result[depth + 1], 16).ToArray());
        return result;
    }
    private static Dictionary<ulong, List<CausalVersion>> Admit(IReadOnlyList<CausalVersion> values, int maxVersions, int maxBytes)
    {
        if (values is null || values.Count > maxVersions) throw Invalid();
        int bytes = 0; var dots = new Dictionary<CausalDot, string>(); var groups = new Dictionary<ulong, List<CausalVersion>>();
        foreach (var input in values)
        {
            var value = Owned(input); string wire = value.Encode(); bytes = checked(bytes + wire.Length);
            if (bytes > maxBytes || dots.TryGetValue(value.Dot, out string? prior) && prior != wire) throw Invalid();
            dots[value.Dot] = wire; if (!groups.TryGetValue(value.Address, out var group)) groups[value.Address] = group = [];
            group.Add(value); if (group.Count > 64) throw Invalid();
        }
        return groups;
    }
    private static byte[] LeafHash(ulong address, IReadOnlyList<CausalVersion> versions)
    {
        if (versions.Count == 0) return Empty[16];
        using var bytes = new MemoryStream(); bytes.Write(Domain); bytes.WriteByte((byte)'L');
        Span<byte> number = stackalloc byte[8]; BinaryPrimitives.WriteUInt64BigEndian(number, address); bytes.Write(number);
        BinaryPrimitives.WriteUInt16BigEndian(number, (ushort)versions.Count); bytes.Write(number[..2]);
        foreach (var value in versions)
        {
            byte[] wire = Encoding.ASCII.GetBytes(value.Encode()); BinaryPrimitives.WriteUInt32BigEndian(number, (uint)wire.Length);
            bytes.Write(number[..4]); bytes.Write(wire);
        }
        return SHA256.HashData(bytes.GetBuffer().AsSpan(0, (int)bytes.Length));
    }
    private static byte[][][] Proof(IReadOnlyList<IReadOnlyList<string>> value, int depth)
    {
        if (value is null || value.Count != depth) throw Invalid();
        return value.Select(level =>
        {
            if (level is null || level.Count != 15) throw Invalid(); return level.Select(Hash).ToArray();
        }).ToArray();
    }
    private static byte[] Fold(byte[] current, string path, byte[][][] proof)
    {
        for (int depth = path.Length - 1; depth >= 0; depth--)
        {
            int digit = int.Parse(path[depth].ToString(), NumberStyles.HexNumber, CultureInfo.InvariantCulture), index = 0;
            var children = new byte[16][];
            for (int i = 0; i < 16; i++) children[i] = i == digit ? current : proof[depth][index++];
            current = Branch(depth, children);
        }
        return current;
    }
    private CausalMerkleSnapshot(Dictionary<ulong, List<CausalVersion>> groups)
    {
        foreach (var (key, group) in groups)
        {
            var merged = CausalVersion.Merge(group, []); _rows[key] = merged;
            _nodes[key.ToString("x16", CultureInfo.InvariantCulture)] = LeafHash(key, merged);
        }
        for (int depth = 15; depth >= 0; depth--)
        {
            string[] parents = _nodes.Keys.Where(path => path.Length == depth + 1).Select(path => path[..depth]).Distinct(StringComparer.Ordinal).ToArray();
            foreach (string parent in parents) _nodes[parent] = Branch(depth, Children(parent));
        }
        Root = Hex(_nodes.GetValueOrDefault("") ?? Empty[0]);
    }
    public static CausalMerkleSnapshot Create(IReadOnlyList<CausalVersion> values, CausalMerkleOptions? options = null)
    {
        options ??= new(); if (options.MaxVersions is < 1 or > 4096 || options.MaxBytes is < 1 or > 8_388_608) throw Invalid();
        return new(Admit(values, options.MaxVersions, options.MaxBytes));
    }
    private byte[][] Children(string path) => Enumerable.Range(0, 16)
        .Select(digit => _nodes.GetValueOrDefault(path + digit.ToString("x", CultureInfo.InvariantCulture)) ?? Empty[path.Length + 1]).ToArray();
    private string[][] PathProof(string path) => path.Select((item, depth) =>
    {
        int digit = int.Parse(item.ToString(), NumberStyles.HexNumber, CultureInfo.InvariantCulture);
        return Children(path[..depth]).Where((_, index) => index != digit).Select(Hex).ToArray();
    }).ToArray();
    public CausalMerkleNode Node(string prefix)
    {
        Prefix(prefix); return new(Root, prefix, Children(prefix).Select(Hex).ToArray(), PathProof(prefix));
    }
    public CausalMerkleLeaf Leaf(ulong address)
    {
        if (address == 0) throw Invalid();
        return new(Root, address, (_rows.GetValueOrDefault(address) ?? []).Select(Owned).ToArray(), PathProof(address.ToString("x16", CultureInfo.InvariantCulture)));
    }
    public static string[] VerifyNode(string expectedRoot, CausalMerkleNode value)
    {
        byte[] expected = Hash(expectedRoot); if (value is null) throw Invalid(); Prefix(value.Prefix);
        if (value.Root != expectedRoot || value.Children is null || value.Children.Count != 16) throw Invalid();
        var children = value.Children.Select(Hash).ToArray(); var proof = Proof(value.Proof, value.Prefix.Length);
        if (!Fold(Branch(value.Prefix.Length, children), value.Prefix, proof).SequenceEqual(expected)) throw Invalid();
        return children.Select(Hex).ToArray();
    }
    public static CausalVersion[] VerifyLeaf(string expectedRoot, CausalMerkleLeaf value)
    {
        byte[] expected = Hash(expectedRoot);
        if (value is null || value.Root != expectedRoot || value.Address == 0) throw Invalid();
        var groups = Admit(value.Versions, 64, 1_048_576); var proof = Proof(value.Proof, 16);
        if (groups.Keys.Any(key => key != value.Address)) throw Invalid();
        var versions = groups.GetValueOrDefault(value.Address)?.ToArray() ?? [];
        if (!versions.Select(v => v.Encode()).SequenceEqual(CausalVersion.Merge(versions, []).Select(v => v.Encode()))) throw Invalid();
        if (!Fold(LeafHash(value.Address, versions), value.Address.ToString("x16", CultureInfo.InvariantCulture), proof).SequenceEqual(expected)) throw Invalid();
        return versions;
    }
    private static JsonElement Frame(string wire, int maxBytes, string tag)
    {
        if (wire is null || wire.Length > maxBytes) throw Invalid();
        int depth = 0, separators = 0; bool quoted = false;
        foreach (char item in wire)
        {
            if (item is < ' ' or > '~' or '\\') throw Invalid();
            if (item == '"') { quoted = !quoted; continue; }
            if (quoted) continue;
            if (item is '{' or '}') throw Invalid();
            if (item == '[') { if (++depth > 5) throw Invalid(); }
            else if (item == ']') { if (--depth < 0) throw Invalid(); }
            else if (item == ',' && ++separators > 32768) throw Invalid();
        }
        if (quoted || depth != 0) throw Invalid();
        using var document = JsonDocument.Parse(wire); var value = document.RootElement;
        if (value.ValueKind != JsonValueKind.Array || value.GetArrayLength() != 5 || Text(value[0]) != tag ||
            JsonSerializer.Serialize(value) != wire) throw Invalid();
        return value.Clone();
    }
    private static string Text(JsonElement value) => value.ValueKind == JsonValueKind.String ? value.GetString()! : throw Invalid();
    private static string[] Strings(JsonElement value, int count)
    {
        if (value.ValueKind != JsonValueKind.Array || value.GetArrayLength() != count) throw Invalid();
        return value.EnumerateArray().Select(Text).ToArray();
    }
    private static string[][] WireProof(JsonElement value, int depth)
    {
        if (value.ValueKind != JsonValueKind.Array || value.GetArrayLength() != depth) throw Invalid();
        return value.EnumerateArray().Select(level => Strings(level, 15)).ToArray();
    }
    public static string EncodeNode(CausalMerkleNode value)
    {
        VerifyNode(value.Root, value);
        return JsonSerializer.Serialize(new object[] { "causal-node1", value.Root, value.Prefix, value.Children, value.Proof });
    }
    public static CausalMerkleNode DecodeNode(string expectedRoot, string wire)
    {
        var value = Frame(wire, 32768, "causal-node1"); string path = Text(value[2]); Prefix(path);
        var result = new CausalMerkleNode(Text(value[1]), path, Strings(value[3], 16), WireProof(value[4], path.Length));
        VerifyNode(expectedRoot, result); return result;
    }
    public static string EncodeLeaf(CausalMerkleLeaf value)
    {
        var versions = VerifyLeaf(value.Root, value).Select(v => JsonSerializer.Deserialize<JsonElement>(v.Encode())).ToArray();
        return JsonSerializer.Serialize(new object[] { "causal-leaf1", value.Root,
            value.Address.ToString(CultureInfo.InvariantCulture), versions, value.Proof });
    }
    public static CausalMerkleLeaf DecodeLeaf(string expectedRoot, string wire)
    {
        var value = Frame(wire, 1_081_344, "causal-leaf1"); string key = Text(value[2]);
        if (key.Length is 0 or > 20 || !ulong.TryParse(key, NumberStyles.None, CultureInfo.InvariantCulture, out ulong address) ||
            address == 0 || address.ToString(CultureInfo.InvariantCulture) != key) throw Invalid();
        if (value[3].ValueKind != JsonValueKind.Array || value[3].GetArrayLength() > 64) throw Invalid();
        var result = new CausalMerkleLeaf(Text(value[1]), address,
            value[3].EnumerateArray().Select(v => CausalVersion.Decode(JsonSerializer.Serialize(v))).ToArray(), WireProof(value[4], 16));
        VerifyLeaf(expectedRoot, result); return result;
    }
    internal string[] ChildHashes(string prefix) => Children(prefix).Select(Hex).ToArray();
    internal static string EmptyHash(int depth) => Hex(Empty[depth]);
    internal static void ValidateRoot(string value) => Hash(value);
}

/// <summary>Owned frontier for the same source root and monotone destination journal.</summary>
public sealed class CausalPullCursor
{
    private readonly string author;
    private readonly string[] pending;
    public string Root { get; }
    internal CausalPullCursor(string root, string author, IEnumerable<string> pending)
    { Root = root; this.author = author; this.pending = pending.ToArray(); }
    internal string[] Resume(string root, string destinationAuthor)
    {
        if (root != Root || author != destinationAuthor) throw new InvalidDataException("Mismatched causal continuation.");
        return pending.ToArray();
    }
}
public sealed record CausalPullOptions(int MaxNodeRequests = 4096, int MaxLeafRequests = 64, int MaxVersionBytes = 8_388_608, CausalPullCursor? Continuation = null);
public sealed record CausalPullProgress(string Root, int NodeRequests, int LeafRequests, int VersionBytes, int ChangedLeaves, bool Complete, CausalPullCursor? Continuation);
public static class CausalMerkleTransfer
{
    public static CausalPullProgress Pull(ICausalMerkleSource source, CausalReplica destination, CausalPullOptions? options = null)
    {
        options ??= new();
        if (source is null || destination is null || options.MaxNodeRequests is < 1 or > 65536 ||
            options.MaxLeafRequests is < 1 or > 4096 || options.MaxVersionBytes is < 1 or > 8_388_608)
            throw new InvalidDataException("Invalid causal pull options.");
        string root = source.Root; CausalMerkleSnapshot.ValidateRoot(root);
        if (options.Continuation is not null && options.Continuation.Root != root) throw new InvalidDataException("Mismatched causal continuation root.");
        var state = destination.Snapshot(); var local = CausalMerkleSnapshot.Create(state.Versions);
        int nodes = 0, leaves = 0, bytes = 0, changed = 0;
        var pending = new Stack<string>(options.Continuation?.Resume(root, state.Author) ?? [""]);
        if (root == local.Root || root == CausalMerkleSnapshot.EmptyHash(0)) pending.Clear();
        CausalPullProgress Progress(bool complete = false) => new(root, nodes, leaves, bytes, changed, complete,
            complete ? null : new CausalPullCursor(root, state.Author, pending.Reverse()));
        CausalPullProgress Paused(string path) { pending.Push(path); return Progress(); }
        while (pending.TryPop(out string? path))
        {
            if (path.Length == 16)
            {
                if (leaves == options.MaxLeafRequests) return Paused(path); leaves++;
                ulong address = ulong.Parse(path, NumberStyles.HexNumber, CultureInfo.InvariantCulture); var response = source.Leaf(address);
                if (response is null || response.Address != address) throw new InvalidDataException("Mismatched causal leaf path.");
                var versions = CausalMerkleSnapshot.VerifyLeaf(root, response); int size = versions.Sum(v => v.Encode().Length);
                if (bytes + size > options.MaxVersionBytes) return Paused(path); bytes += size;
                if (destination.Receive(versions)) changed++;
            }
            else
            {
                if (nodes == options.MaxNodeRequests) return Paused(path); nodes++; var response = source.Node(path);
                if (response is null || response.Prefix != path) throw new InvalidDataException("Mismatched causal node path.");
                var remote = CausalMerkleSnapshot.VerifyNode(root, response); var current = local.ChildHashes(path);
                for (int digit = 15; digit >= 0; digit--)
                    if (remote[digit] != current[digit] && remote[digit] != CausalMerkleSnapshot.EmptyHash(path.Length + 1))
                        pending.Push(path + digit.ToString("x", CultureInfo.InvariantCulture));
            }
        }
        return Progress(true);
    }
}
