using System.Globalization;
using System.Text;

using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>
/// Optional features of the binary LiNo protocol. Every feature is off by
/// default; each one can be switched on independently, like stacking a decorator.
/// </summary>
public readonly record struct BinaryLinoOptions
{
    // Doublets are stored as null so that default(BinaryLinoOptions) means doublets.
    private readonly ArityRange? _arity;

    /// <summary>
    /// Send numbers and code points as Hybrid external references instead of
    /// in-band unary links. Halves the internal address range of each width.
    /// </summary>
    public bool ExternalReferences { get; init; }

    /// <summary>
    /// The link lengths the encoder may use. The default, exactly 2, sends
    /// only doublets; <c>2..3</c> adds triplets and <c>1..</c> any length.
    /// The range must include 2.
    /// </summary>
    public ArityRange Arity
    {
        get => _arity ?? ArityRange.Doublets;
        init => _arity = value == ArityRange.Doublets ? null : value;
    }

    /// <summary>
    /// Give every section of the packet the narrowest width its links need
    /// instead of one width for the whole packet.
    /// </summary>
    public bool PackedWidths { get; init; }

    /// <summary>Enables or disables external references.</summary>
    public BinaryLinoOptions WithExternalReferences(bool enabled = true) => this with { ExternalReferences = enabled };

    /// <summary>Sets the link lengths the encoder may use.</summary>
    public BinaryLinoOptions WithArity(ArityRange arity) => this with { Arity = arity };

    /// <summary>Enables or disables packed widths.</summary>
    public BinaryLinoOptions WithPackedWidths(bool enabled = true) => this with { PackedWidths = enabled };

    /// <summary>The options a peer most likely used to write <paramref name="packet"/>, so a reply can be written in the same style.</summary>
    public static BinaryLinoOptions OfPacket(LinksPacket packet)
    {
        ArgumentNullException.ThrowIfNull(packet);
        ulong shortest = 2;
        ulong longest = 2;
        foreach (var (_, link) in packet.Links())
        {
            shortest = Math.Min(shortest, (ulong)link.Length);
            longest = Math.Max(longest, (ulong)link.Length);
        }
        return new BinaryLinoOptions
        {
            ExternalReferences = packet.ExternalReferences,
            Arity = ArityRange.Between(shortest, longest),
            PackedWidths = packet.Sections.Select(section => section.Width).Distinct().Skip(1).Any(),
        };
    }

    /// <summary>Lists every option, for test and error messages.</summary>
    public override string ToString() =>
        $"BinaryLinoOptions {{ ExternalReferences = {ExternalReferences}, Arity = {Arity}, PackedWidths = {PackedWidths} }}";
}

/// <summary>
/// Lossless mapping between LiNo documents and <see cref="LinksPacket"/>s,
/// byte-for-byte compatible with the Rust port.
/// </summary>
/// <remarks>
/// <list type="bullet">
/// <item><c>()</c> is the null link 0.</item>
/// <item>A numeric reference n is <c>(Number unary(n))</c>, or an external reference when enabled.</item>
/// <item>Any other reference is <c>(String code points…)</c>; code points are unary numbers or externals.</item>
/// <item>A link without an id and with exactly two values is a plain doublet.</item>
/// <item>A link without an id and any other number of values is a list.</item>
/// <item>A link with an id is <c>(Identified id values…)</c>.</item>
/// <item>The document is a list of its top-level links, stored last (the root).</item>
/// </list>
/// A list or typed value with any number of values other than two is a
/// single link of that many references when <see cref="BinaryLinoOptions.Arity"/>
/// allows it (<c>[marker, elements…]</c> for typed values, the bare elements
/// for lists), and otherwise the doublet <c>(marker chain)</c>, where chain is
/// the nil-terminated cons list <c>(e1 (e2 (… (en 0))))</c>. Identical
/// sub-links are emitted once and shared. Links are numbered so that every
/// link only refers to earlier ones: doublets of plain doublets first, then
/// the rest in creation order.
/// </remarks>
public static class LinoMapping
{
    /// <summary>Converts a document into a packet.</summary>
    public static LinksPacket EncodeDocument(IReadOnlyList<LinoLink> document, BinaryLinoOptions options = default)
    {
        ArgumentNullException.ThrowIfNull(document);
        if (!options.Arity.Contains(2))
        {
            throw LinoProtocolException.Unencodable($"arity {options.Arity} does not include doublets (2)");
        }
        var encoder = new Encoder(options);
        if (document.Count > 0)
        {
            encoder.List(document.Select(encoder.Encode).ToList());
        }
        return encoder.Finish();
    }

    /// <summary>Converts a packet back into a document.</summary>
    public static IReadOnlyList<LinoLink> DecodeDocument(LinksPacket packet, DecodeLimits? limits = null)
    {
        ArgumentNullException.ThrowIfNull(packet);
        limits ??= DecodeLimits.Default;
        var decoder = new Decoder(packet, limits);
        if (decoder.LinkCount == 0)
        {
            return Array.Empty<LinoLink>();
        }
        var root = decoder.View(PacketReference.Internal(LinksPacket.FirstLinkAddress + (ulong)decoder.LinkCount - 1));
        IReadOnlyList<PacketReference> items = root switch
        {
            { Kind: ViewKind.Link, Items: [{ IsExternal: false, Value: LinksPacket.List }, var chain] } => decoder.Chain(chain),
            { Kind: ViewKind.Link } when !StartsWithMarker(root.Items!) => root.Items!,
            _ => throw LinoProtocolException.Malformed("the root link is not a list"),
        };
        var budget = limits.MaxNodes;
        return items.Select(item => decoder.Decode(item, 0, ref budget)).ToList();
    }

    /// <summary>Parses a canonical unsigned decimal number (no sign, no leading zeros).</summary>
    public static bool TryParseCanonicalNumber(string text, out ulong value)
    {
        value = 0;
        var canonical = !string.IsNullOrEmpty(text)
            && text.All(character => character is >= '0' and <= '9')
            && (text == "0" || text[0] != '0');
        return canonical && ulong.TryParse(text, NumberStyles.None, CultureInfo.InvariantCulture, out value);
    }

    private static bool IsMarker(PacketReference reference) =>
        !reference.IsExternal && reference.Value is >= LinksPacket.One and < LinksPacket.FirstLinkAddress;

    private static bool StartsWithMarker(IReadOnlyList<PacketReference> items) => items.Count > 0 && IsMarker(items[0]);

    private enum NodeKind
    {
        Internal,
        External,
        /// <summary>A doublet of plain doublets, markers and externals.</summary>
        Doublet,
        /// <summary>Any other link; numbered after every doublet.</summary>
        Tuple,
    }

    private readonly record struct Node(NodeKind Kind, ulong Value)
    {
        public static Node Internal(ulong address) => new(NodeKind.Internal, address);

        public static Node External(ulong value) => new(NodeKind.External, value);
    }

    private sealed class NodeListComparer : IEqualityComparer<List<Node>>
    {
        public static readonly NodeListComparer Instance = new();

        public bool Equals(List<Node>? x, List<Node>? y) =>
            ReferenceEquals(x, y) || (x is not null && y is not null && x.SequenceEqual(y));

        public int GetHashCode(List<Node> obj)
        {
            var hash = new HashCode();
            foreach (var node in obj)
            {
                hash.Add(node);
            }
            return hash.ToHashCode();
        }
    }

    private sealed class Encoder
    {
        private readonly BinaryLinoOptions _options;
        private readonly List<List<Node>> _doublets = new();
        private readonly List<List<Node>> _tuples = new();
        private readonly Dictionary<List<Node>, Node> _created = new(NodeListComparer.Instance);
        private readonly List<Node> _powers = new() { Node.Internal(LinksPacket.One) };

        public Encoder(BinaryLinoOptions options) => _options = options;

        private Node Link(List<Node> items)
        {
            if (_created.TryGetValue(items, out var existing))
            {
                return existing;
            }
            Node node;
            if (items.Count == 2 && items.All(item => item.Kind != NodeKind.Tuple))
            {
                _doublets.Add(items);
                node = new Node(NodeKind.Doublet, (ulong)_doublets.Count - 1);
            }
            else
            {
                _tuples.Add(items);
                node = new Node(NodeKind.Tuple, (ulong)_tuples.Count - 1);
            }
            _created[items] = node;
            return node;
        }

        private Node Pair(Node first, Node second) => Link(new List<Node> { first, second });

        private Node Chain(IReadOnlyList<Node> items)
        {
            var tail = Node.Internal(LinksPacket.NullAddress);
            for (var index = items.Count - 1; index >= 0; index--)
            {
                tail = Pair(items[index], tail);
            }
            return tail;
        }

        // One link of `items` when the arity allows it, except that two items
        // are always a doublet.
        private bool FitsOneLink(int items) => items != 2 && _options.Arity.Contains((ulong)items);

        private Node Typed(ulong marker, List<Node> elements)
        {
            if (FitsOneLink(elements.Count + 1))
            {
                elements.Insert(0, Node.Internal(marker));
                return Link(elements);
            }
            return Pair(Node.Internal(marker), Chain(elements));
        }

        public Node List(List<Node> elements) => elements.Count switch
        {
            0 => Node.Internal(LinksPacket.NullAddress),
            2 => Pair(elements[0], elements[1]),
            _ when FitsOneLink(elements.Count) => Link(elements),
            _ => Typed(LinksPacket.List, elements),
        };

        private Node Power(int exponent)
        {
            while (_powers.Count <= exponent)
            {
                var previous = _powers[^1];
                _powers.Add(Pair(previous, previous));
            }
            return _powers[exponent];
        }

        private Node Unary(ulong value)
        {
            var powers = new List<Node>();
            for (var bit = 63; bit >= 0; bit--)
            {
                if ((value & (1UL << bit)) != 0)
                {
                    powers.Add(Power(bit));
                }
            }
            if (powers.Count == 0)
            {
                return Node.Internal(LinksPacket.NullAddress);
            }
            var sum = powers[^1];
            for (var index = powers.Count - 2; index >= 0; index--)
            {
                sum = Pair(powers[index], sum);
            }
            return sum;
        }

        private bool IsExternal(ulong value) => _options.ExternalReferences && value <= LinksPacket.ExternalCapacity(8);

        private Node Scalar(ulong value) => IsExternal(value) ? Node.External(value) : Unary(value);

        private Node Reference(string text)
        {
            if (TryParseCanonicalNumber(text, out var value))
            {
                return IsExternal(value) ? Node.External(value) : Pair(Node.Internal(LinksPacket.Number), Unary(value));
            }
            var codePoints = text.EnumerateRunes().Select(rune => Scalar((ulong)rune.Value)).ToList();
            return Typed(LinksPacket.String, codePoints);
        }

        public Node Encode(LinoLink link)
        {
            if (link.Values is not { } values)
            {
                return Reference(link.Id ?? string.Empty);
            }
            if (link.Id is { } id)
            {
                var elements = new List<Node> { Reference(id) };
                elements.AddRange(values.Select(Encode));
                return Typed(LinksPacket.Identified, elements);
            }
            return List(values.Select(Encode).ToList());
        }

        public LinksPacket Finish()
        {
            var doubletCount = (ulong)_doublets.Count;
            PacketReference Resolve(Node node) => node.Kind switch
            {
                NodeKind.Internal => PacketReference.Internal(node.Value),
                NodeKind.External => PacketReference.External(node.Value),
                NodeKind.Doublet => PacketReference.Internal(LinksPacket.FirstLinkAddress + node.Value),
                _ => PacketReference.Internal(LinksPacket.FirstLinkAddress + doubletCount + node.Value),
            };
            var links = _doublets
                .Concat(_tuples)
                .Select((items, index) => (LinksPacket.FirstLinkAddress + (ulong)index, items.Select(Resolve).ToArray()))
                .ToList();
            return LinksPacket.Pack(_options.ExternalReferences, links, _options.PackedWidths);
        }
    }

    private enum ViewKind
    {
        Null,
        Marker,
        External,
        Link,
    }

    private readonly record struct View(ViewKind Kind, ulong Value = 0, PacketReference[]? Items = null);

    private sealed class Decoder
    {
        private readonly DecodeLimits _limits;
        // _links[i] is the link at address FirstLinkAddress + i.
        private readonly List<PacketReference[]> _links = new();
        // _unary[i] is the number link i denotes, if it is a unary number.
        private readonly List<ulong?> _unary = new();

        public Decoder(LinksPacket packet, DecodeLimits limits)
        {
            _limits = limits;
            foreach (var (address, link) in packet.Links())
            {
                var expected = LinksPacket.FirstLinkAddress + (ulong)_links.Count;
                if (address != expected)
                {
                    throw LinoProtocolException.Malformed(
                        $"a LiNo packet stores its links contiguously from address {LinksPacket.FirstLinkAddress}, " +
                        $"found link {address} where {expected} belongs");
                }
                foreach (var reference in link)
                {
                    if (!reference.IsExternal && reference.Value >= address)
                    {
                        throw LinoProtocolException.Malformed(
                            $"link {address} refers to {reference}, which is not an earlier link");
                    }
                }
                // Links only refer backwards, so one forward pass evaluates every
                // unary number without recursion.
                ulong? value = null;
                if (link.Length == 2
                    && UnaryValue(link[0]) is { } source
                    && UnaryValue(link[1]) is { } target
                    && source <= ulong.MaxValue - target)
                {
                    value = source + target;
                }
                _unary.Add(value);
                _links.Add(link);
            }
        }

        public int LinkCount => _links.Count;

        private ulong? UnaryValue(PacketReference reference) => reference switch
        {
            { IsExternal: true } => null,
            { Value: LinksPacket.NullAddress } => 0,
            { Value: LinksPacket.One } => 1,
            { Value: >= LinksPacket.FirstLinkAddress } => _unary[(int)(reference.Value - LinksPacket.FirstLinkAddress)],
            _ => null,
        };

        public View View(PacketReference reference) => reference switch
        {
            { IsExternal: true } => new View(ViewKind.External, reference.Value),
            { Value: LinksPacket.NullAddress } => new View(ViewKind.Null),
            { Value: < LinksPacket.FirstLinkAddress } => new View(ViewKind.Marker, reference.Value),
            _ => new View(ViewKind.Link, Items: _links[(int)(reference.Value - LinksPacket.FirstLinkAddress)]),
        };

        private ulong Number(PacketReference reference) =>
            reference.IsExternal
                ? reference.Value
                : UnaryValue(reference) ?? throw LinoProtocolException.Malformed("expected a unary number");

        /// <summary>The elements of the cons list <c>(e1 (e2 (… (en 0))))</c>.</summary>
        public List<PacketReference> Chain(PacketReference tail)
        {
            var elements = new List<PacketReference>();
            while (true)
            {
                var view = View(tail);
                switch (view)
                {
                    case { Kind: ViewKind.Null }:
                        return elements;
                    case { Kind: ViewKind.Link, Items: [var head, var next] }:
                        if (elements.Count >= _limits.MaxNodes)
                        {
                            throw LinoProtocolException.Limit("chain too long");
                        }
                        elements.Add(head);
                        tail = next;
                        break;
                    default:
                        throw LinoProtocolException.Malformed("broken element chain");
                }
            }
        }

        private LinoLink Typed(ulong marker, IReadOnlyList<PacketReference> elements, int depth, ref long budget)
        {
            switch (marker)
            {
                case LinksPacket.Number:
                    if (elements.Count != 1)
                    {
                        throw LinoProtocolException.Malformed("a number needs exactly one value");
                    }
                    return LinoFormat.Reference(Number(elements[0]).ToString(CultureInfo.InvariantCulture));
                case LinksPacket.String:
                    var text = new StringBuilder(elements.Count);
                    foreach (var element in elements)
                    {
                        var codePoint = Number(element);
                        if (codePoint > int.MaxValue || !Rune.IsValid((int)codePoint))
                        {
                            throw LinoProtocolException.Malformed($"invalid code point {codePoint}");
                        }
                        text.Append(new Rune((int)codePoint).ToString());
                    }
                    return LinoFormat.Reference(text.ToString());
                case LinksPacket.List:
                    return List(elements, 0, depth, ref budget);
                case LinksPacket.Identified:
                    if (elements.Count == 0)
                    {
                        throw LinoProtocolException.Malformed("an identified link needs an id");
                    }
                    var id = Decode(elements[0], depth + 1, ref budget);
                    if (id.Values is not null)
                    {
                        throw LinoProtocolException.Malformed("a link id must be a reference");
                    }
                    var values = List(elements, 1, depth, ref budget).Values!;
                    return LinoFormat.Link(id.Id ?? string.Empty, values);
                default:
                    throw LinoProtocolException.Malformed($"marker {marker} cannot start a typed value");
            }
        }

        private LinoLink List(IReadOnlyList<PacketReference> elements, int skip, int depth, ref long budget)
        {
            var values = new List<LinoLink>(Math.Max(0, elements.Count - skip));
            for (var index = skip; index < elements.Count; index++)
            {
                values.Add(Decode(elements[index], depth + 1, ref budget));
            }
            return LinoFormat.Link(null, values);
        }

        public LinoLink Decode(PacketReference reference, int depth, ref long budget)
        {
            if (depth >= _limits.MaxDepth)
            {
                throw LinoProtocolException.Limit($"nesting deeper than {_limits.MaxDepth}");
            }
            if (budget <= 0)
            {
                throw LinoProtocolException.Limit("too many LiNo nodes");
            }
            budget--;
            var view = View(reference);
            switch (view)
            {
                case { Kind: ViewKind.Null }:
                    return LinoFormat.Link(null, new List<LinoLink>());
                case { Kind: ViewKind.External }:
                    return LinoFormat.Reference(view.Value.ToString(CultureInfo.InvariantCulture));
                case { Kind: ViewKind.Marker }:
                    throw LinoProtocolException.Malformed($"marker {view.Value} used as a value");
                case { Items: [{ IsExternal: false, Value: LinksPacket.Number }, var value] }:
                    return Typed(LinksPacket.Number, new[] { value }, depth, ref budget);
                case { Items: [var marker, var chain] } when IsMarker(marker):
                    return Typed(marker.Value, Chain(chain), depth, ref budget);
                default:
                    var items = view.Items!;
                    return StartsWithMarker(items)
                        ? Typed(items[0].Value, items[1..], depth, ref budget)
                        : List(items, 0, depth, ref budget);
            }
        }
    }
}
