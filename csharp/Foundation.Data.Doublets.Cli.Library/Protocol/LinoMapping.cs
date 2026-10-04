using System.Globalization;
using System.Text;

using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>
/// Optional features of the binary LiNo protocol. Every feature is off by
/// default; each one can be switched on independently, like stacking a decorator.
/// </summary>
/// <param name="ExternalReferences">
/// Send numbers and code points as Hybrid external references instead of
/// in-band unary links. Halves the internal address range of each width.
/// </param>
/// <param name="Sequences">
/// Use the variable-length sequence section for lists, strings and links with
/// ids instead of cons chains of doublets.
/// </param>
/// <param name="ProgressiveWidths">
/// Let the reference width grow with the address instead of using the width of
/// the largest address for every reference.
/// </param>
public readonly record struct BinaryLinoOptions(
    bool ExternalReferences = false,
    bool Sequences = false,
    bool ProgressiveWidths = false)
{
    /// <summary>Enables or disables external references.</summary>
    public BinaryLinoOptions WithExternalReferences(bool enabled = true) => this with { ExternalReferences = enabled };

    /// <summary>Enables or disables the sequence section.</summary>
    public BinaryLinoOptions WithSequences(bool enabled = true) => this with { Sequences = enabled };

    /// <summary>Enables or disables progressive reference widths.</summary>
    public BinaryLinoOptions WithProgressiveWidths(bool enabled = true) => this with { ProgressiveWidths = enabled };
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
/// Typed values and lists are a sequence <c>[marker, elements…]</c> when the
/// sequence section is enabled and the doublet <c>(marker chain)</c> otherwise,
/// where chain is the nil-terminated cons list <c>(e1 (e2 (… (en 0))))</c>.
/// Identical sub-links are emitted once and shared.
/// </remarks>
public static class LinoMapping
{
    /// <summary>Converts a document into a packet.</summary>
    public static LinksPacket EncodeDocument(IReadOnlyList<LinoLink> document, BinaryLinoOptions options = default)
    {
        ArgumentNullException.ThrowIfNull(document);
        var encoder = new Encoder(options);
        if (document.Count > 0)
        {
            var items = document.Select(encoder.Encode).ToList();
            if (options.Sequences)
            {
                encoder.Sequences.Add(items);
            }
            else
            {
                var chain = encoder.Chain(items);
                encoder.Doublets.Add((Node.Internal(LinksPacket.List), chain));
            }
        }
        return encoder.Finish();
    }

    /// <summary>Converts a packet back into a document.</summary>
    public static IReadOnlyList<LinoLink> DecodeDocument(LinksPacket packet, DecodeLimits? limits = null)
    {
        ArgumentNullException.ThrowIfNull(packet);
        limits ??= DecodeLimits.Default;
        if (packet.LastAddress is not { } root)
        {
            return Array.Empty<LinoLink>();
        }
        var decoder = new Decoder(packet, limits);
        var view = decoder.View(PacketReference.Internal(root));
        IReadOnlyList<PacketReference> items = view switch
        {
            { Kind: ViewKind.Sequence } when !StartsWithMarker(view.Items!) => view.Items!,
            { Kind: ViewKind.Doublet, Source: { IsExternal: false, Value: LinksPacket.List } } => decoder.Chain(view.Target),
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

    private static bool IsMarker(ulong address) => address is >= LinksPacket.One and < LinksPacket.FirstLinkAddress;

    private static bool StartsWithMarker(IReadOnlyList<PacketReference> items) =>
        items.Count > 0 && !items[0].IsExternal && IsMarker(items[0].Value);

    private enum NodeKind
    {
        Internal,
        External,
        Doublet,
        Sequence,
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
        private readonly Dictionary<(Node, Node), int> _doubletIndex = new();
        private readonly Dictionary<List<Node>, int> _sequenceIndex = new(NodeListComparer.Instance);
        private readonly List<Node> _powers = new() { Node.Internal(LinksPacket.One) };

        public Encoder(BinaryLinoOptions options) => _options = options;

        public List<(Node Source, Node Target)> Doublets { get; } = new();

        public List<List<Node>> Sequences { get; } = new();

        private Node Doublet(Node source, Node target)
        {
            if (_doubletIndex.TryGetValue((source, target), out var existing))
            {
                return new Node(NodeKind.Doublet, (ulong)existing);
            }
            var index = Doublets.Count;
            Doublets.Add((source, target));
            _doubletIndex[(source, target)] = index;
            return new Node(NodeKind.Doublet, (ulong)index);
        }

        private Node Sequence(List<Node> items)
        {
            if (_sequenceIndex.TryGetValue(items, out var existing))
            {
                return new Node(NodeKind.Sequence, (ulong)existing);
            }
            var index = Sequences.Count;
            Sequences.Add(items);
            _sequenceIndex[items] = index;
            return new Node(NodeKind.Sequence, (ulong)index);
        }

        // Fixed doublets may only refer to fixed doublets, so a pair holding
        // a sequence becomes a two-element sequence.
        private Node Pair(Node first, Node second) =>
            first.Kind == NodeKind.Sequence || second.Kind == NodeKind.Sequence
                ? Sequence(new List<Node> { first, second })
                : Doublet(first, second);

        public Node Chain(IReadOnlyList<Node> items)
        {
            var tail = Node.Internal(LinksPacket.NullAddress);
            for (var index = items.Count - 1; index >= 0; index--)
            {
                tail = Doublet(items[index], tail);
            }
            return tail;
        }

        private Node Typed(ulong marker, List<Node> elements)
        {
            if (_options.Sequences)
            {
                elements.Insert(0, Node.Internal(marker));
                return Sequence(elements);
            }
            return Doublet(Node.Internal(marker), Chain(elements));
        }

        private Node Power(int exponent)
        {
            while (_powers.Count <= exponent)
            {
                var previous = _powers[^1];
                _powers.Add(Doublet(previous, previous));
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
                sum = Doublet(powers[index], sum);
            }
            return sum;
        }

        private Node Scalar(ulong value) =>
            _options.ExternalReferences && value <= LinksPacket.ExternalCapacity(8) ? Node.External(value) : Unary(value);

        private Node Reference(string text)
        {
            if (TryParseCanonicalNumber(text, out var value))
            {
                if (_options.ExternalReferences && value <= LinksPacket.ExternalCapacity(8))
                {
                    return Node.External(value);
                }
                var unary = Unary(value);
                return Doublet(Node.Internal(LinksPacket.Number), unary);
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
            switch (values.Count)
            {
                case 0:
                    return Node.Internal(LinksPacket.NullAddress);
                case 2:
                    var first = Encode(values[0]);
                    var second = Encode(values[1]);
                    return Pair(first, second);
                default:
                    var items = values.Select(Encode).ToList();
                    return _options.Sequences ? Sequence(items) : Typed(LinksPacket.List, items);
            }
        }

        public LinksPacket Finish()
        {
            var doubletCount = (ulong)Doublets.Count;
            PacketReference Resolve(Node node) => node.Kind switch
            {
                NodeKind.Internal => PacketReference.Internal(node.Value),
                NodeKind.External => PacketReference.External(node.Value),
                NodeKind.Doublet => PacketReference.Internal(LinksPacket.FirstLinkAddress + node.Value),
                _ => PacketReference.Internal(LinksPacket.FirstLinkAddress + doubletCount + node.Value),
            };
            var packet = new LinksPacket
            {
                ExternalReferences = _options.ExternalReferences,
                SequencesSection = _options.Sequences,
            };
            packet.Doublets.AddRange(Doublets.Select(doublet => (Resolve(doublet.Source), Resolve(doublet.Target))));
            packet.Sequences.AddRange(Sequences.Select(items => items.Select(Resolve).ToList()));
            packet.MinWidth = packet.RequiredMinWidth(!_options.ProgressiveWidths);
            return packet;
        }
    }

    private enum ViewKind
    {
        Null,
        Marker,
        External,
        Doublet,
        Sequence,
    }

    private readonly record struct View(
        ViewKind Kind,
        ulong Value = 0,
        PacketReference Source = default,
        PacketReference Target = default,
        IReadOnlyList<PacketReference>? Items = null);

    private sealed class Decoder
    {
        private readonly LinksPacket _packet;
        private readonly DecodeLimits _limits;
        // _unary[i] is the number doublet i denotes, if it is a unary number.
        private readonly ulong?[] _unary;

        public Decoder(LinksPacket packet, DecodeLimits limits)
        {
            _packet = packet;
            _limits = limits;
            // Links only refer backwards, so one forward pass evaluates every
            // unary number without recursion.
            _unary = new ulong?[packet.Doublets.Count];
            for (var index = 0; index < packet.Doublets.Count; index++)
            {
                var (source, target) = packet.Doublets[index];
                if (UnaryValue(source) is { } sourceValue
                    && UnaryValue(target) is { } targetValue
                    && sourceValue <= ulong.MaxValue - targetValue)
                {
                    _unary[index] = sourceValue + targetValue;
                }
            }
        }

        private ulong? UnaryValue(PacketReference reference)
        {
            if (reference.IsExternal)
            {
                return null;
            }
            return reference.Value switch
            {
                LinksPacket.NullAddress => 0,
                LinksPacket.One => 1,
                >= LinksPacket.FirstLinkAddress when reference.Value - LinksPacket.FirstLinkAddress < (ulong)_unary.Length
                    => _unary[reference.Value - LinksPacket.FirstLinkAddress],
                _ => null,
            };
        }

        public View View(PacketReference reference)
        {
            if (reference.IsExternal)
            {
                return new View(ViewKind.External, reference.Value);
            }
            if (reference.Value == LinksPacket.NullAddress)
            {
                return new View(ViewKind.Null);
            }
            if (reference.Value < LinksPacket.FirstLinkAddress)
            {
                return new View(ViewKind.Marker, reference.Value);
            }
            var address = reference.Value - LinksPacket.FirstLinkAddress;
            var doublets = (ulong)_packet.Doublets.Count;
            if (address < doublets)
            {
                var (source, target) = _packet.Doublets[(int)address];
                return new View(ViewKind.Doublet, Source: source, Target: target);
            }
            if (address - doublets < (ulong)_packet.Sequences.Count)
            {
                return new View(ViewKind.Sequence, Items: _packet.Sequences[(int)(address - doublets)]);
            }
            throw LinoProtocolException.Malformed($"dangling reference {address}");
        }

        private ulong Number(PacketReference reference)
        {
            if (reference.IsExternal)
            {
                return reference.Value;
            }
            return UnaryValue(reference) ?? throw LinoProtocolException.Malformed("expected a unary number");
        }

        // The elements of a typed value given in doublet form: a cons chain,
        // optionally ending in a sequence holding the remaining elements.
        public List<PacketReference> Chain(PacketReference tail)
        {
            var elements = new List<PacketReference>();
            while (true)
            {
                var view = View(tail);
                switch (view.Kind)
                {
                    case ViewKind.Null:
                        return elements;
                    case ViewKind.Doublet:
                        if (elements.Count >= _limits.MaxNodes)
                        {
                            throw LinoProtocolException.Limit("chain too long");
                        }
                        elements.Add(view.Source);
                        tail = view.Target;
                        break;
                    case ViewKind.Sequence:
                        elements.AddRange(view.Items!);
                        return elements;
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
            switch (view.Kind)
            {
                case ViewKind.Null:
                    return LinoFormat.Link(null, new List<LinoLink>());
                case ViewKind.External:
                    return LinoFormat.Reference(view.Value.ToString(CultureInfo.InvariantCulture));
                case ViewKind.Marker:
                    throw LinoProtocolException.Malformed($"marker {view.Value} used as a value");
                case ViewKind.Doublet when !view.Source.IsExternal && view.Source.Value == LinksPacket.Number:
                    return Typed(LinksPacket.Number, new[] { view.Target }, depth, ref budget);
                case ViewKind.Doublet when !view.Source.IsExternal && IsMarker(view.Source.Value):
                    return Typed(view.Source.Value, Chain(view.Target), depth, ref budget);
                case ViewKind.Doublet:
                    return List(new[] { view.Source, view.Target }, 0, depth, ref budget);
                default:
                    var items = view.Items!;
                    return StartsWithMarker(items)
                        ? Typed(items[0].Value, items.Skip(1).ToList(), depth, ref budget)
                        : List(items, 0, depth, ref budget);
            }
        }
    }
}
