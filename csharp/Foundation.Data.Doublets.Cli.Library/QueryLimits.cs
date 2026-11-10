using System.Globalization;
using System.Numerics;
using System.Text;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli;

/// <summary>Finite admission profile used by checked causal queries. Ordinary stores opt in.</summary>
public sealed class QueryLimits
{
    public int MaxInputBytes => 1024 * 1024;
    public int MaxDepth => 64;
    public int MaxNodes => 4096;
    public int MaxResults => 4096;
    public int MaxBindings => 16384;
    public int MaxBindingBytes => 4 * 1024 * 1024;
    public int MaxReplyBytes => 1024 * 1024;
    public UInt128 MaxAddress => ulong.MaxValue;

    internal void Input(string? text)
    {
        if (text is not null && (text.Length > MaxInputBytes || Encoding.UTF8.GetByteCount(text) > MaxInputBytes))
            throw new InvalidOperationException("Query input byte limit exceeded.");
    }
    internal void Nodes(IEnumerable<LinoLink> roots)
    {
        var pending = new Stack<LinoLink>(roots); var count = 0;
        while (pending.TryPop(out var node))
        {
            if (++count > MaxNodes) throw new InvalidOperationException("Query node limit exceeded.");
            if (node.Values is { } children) foreach (var child in children) pending.Push(child);
        }
    }
    internal void Document(IReadOnlyList<LinoLink> roots)
    {
        if (roots.Count > MaxNodes) throw new InvalidOperationException("Protocol node limit exceeded.");
        var pending = new Stack<(LinoLink Node, int Depth)>(roots.Select(node => (node, 0)));
        var count = roots.Count; long bytes = 0;
        while (pending.TryPop(out var item))
        {
            if (item.Depth > MaxDepth) throw new InvalidOperationException("Protocol depth limit exceeded.");
            ReferenceBytes(item.Node.Id, ref bytes, MaxInputBytes);
            if (item.Node.Values is not { } children) continue;
            if (children.Count > MaxNodes - count) throw new InvalidOperationException("Protocol node limit exceeded.");
            count += children.Count;
            foreach (var child in children) pending.Push((child, item.Depth + 1));
        }
    }
    internal void DecodedReply(IReadOnlyList<LinoLink> roots)
    {
        var pending = new Stack<LinoLink>(roots); long bytes = 0;
        while (pending.TryPop(out var node))
        {
            ReferenceBytes(node.Id, ref bytes, MaxReplyBytes);
            if (node.Values is { } children) foreach (var child in children) pending.Push(child);
        }
    }
    private static void ReferenceBytes(string? value, ref long bytes, int limit)
    {
        if (value is null) return;
        if (value.Length > limit) throw new InvalidOperationException("Protocol decoded byte limit exceeded.");
        bytes += Encoding.UTF8.GetByteCount(value);
        if (bytes > limit) throw new InvalidOperationException("Protocol decoded byte limit exceeded.");
    }
    internal void Numeric(string value)
    {
        var digits = value.AsSpan();
        if (digits.Length > 0 && (digits[0] == '-' || digits[0] == '+')) digits = digits[1..];
        if (digits.Length == 0) return;
        foreach (var digit in digits) if (!char.IsAsciiDigit(digit)) return;
        if (value[0] == '-' || !UInt128.TryParse(value, NumberStyles.Integer, CultureInfo.InvariantCulture, out var number) || number > MaxAddress)
            throw new InvalidOperationException("Unsupported query address.");
    }
    internal void Results(int count)
    {
        if (count > MaxResults) throw new InvalidOperationException("Query result limit exceeded.");
    }
    internal void Bindings<T>(Dictionary<string, T> previous, Dictionary<string, T> assignments, ref int count, ref long bytes)
    {
        foreach (var key in previous.Keys.Concat(assignments.Keys).Distinct(StringComparer.Ordinal))
        {
            count++;
            bytes += Encoding.UTF8.GetByteCount(key) + 80L;
            if (count > MaxBindings || bytes > MaxBindingBytes) throw new InvalidOperationException("Query binding limit exceeded.");
        }
    }
    internal void Reply<T>(INamedTypesLinks<T> links, IEnumerable<(LinkParts<T> Before, LinkParts<T> After)> changes)
        where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
    {
        long bytes = 0;
        foreach (var (before, after) in changes)
            foreach (var row in new[] { before, after })
                foreach (var id in row)
                {
                    bytes += Encoding.UTF8.GetByteCount(links.GetName(id) ?? id.ToString()!);
                    if (bytes > MaxReplyBytes) throw new InvalidOperationException("Query decoded reply byte limit exceeded.");
                }
    }
}
