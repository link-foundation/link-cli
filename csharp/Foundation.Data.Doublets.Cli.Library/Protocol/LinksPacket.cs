using System.Buffers.Binary;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>One reference inside a <see cref="LinksPacket"/>.</summary>
/// <param name="IsExternal">True for an external value (a number or a code point).</param>
/// <param name="Value">The address inside the packet, or the external value.</param>
public readonly record struct PacketReference(bool IsExternal, ulong Value)
{
    /// <summary>The null reference.</summary>
    public static readonly PacketReference Null = Internal(LinksPacket.NullAddress);

    /// <summary>An address inside the packet (0 null, 1..5 markers, then links).</summary>
    public static PacketReference Internal(ulong address) => new(false, address);

    /// <summary>An external value, e.g. a number or a Unicode code point.</summary>
    public static PacketReference External(ulong value) => new(true, value);

    public override string ToString() => IsExternal ? $"External({Value})" : $"Internal({Value})";
}

/// <summary>Safety limits applied while decoding untrusted input.</summary>
public sealed record DecodeLimits
{
    /// <summary>Maximum number of links in a packet.</summary>
    public ulong MaxLinks { get; init; } = 1UL << 22;

    /// <summary>Maximum number of references in all links of a packet.</summary>
    public ulong MaxReferences { get; init; } = 1UL << 24;

    /// <summary>Maximum number of LiNo nodes a packet may expand to.</summary>
    public long MaxNodes { get; init; } = 1L << 22;

    /// <summary>Maximum LiNo nesting depth.</summary>
    public int MaxDepth { get; init; } = 1024;

    /// <summary>Maximum size of a text message in bytes.</summary>
    public long MaxTextBytes { get; init; } = 64L << 20;

    /// <summary>The default limits.</summary>
    public static DecodeLimits Default { get; } = new();

    /// <summary>Limits for trusted input such as a store archive: only the address space bounds the packet.</summary>
    public static DecodeLimits Unlimited { get; } = new()
    {
        MaxLinks = ulong.MaxValue,
        MaxReferences = ulong.MaxValue,
        MaxNodes = long.MaxValue,
        MaxDepth = int.MaxValue,
        MaxTextBytes = long.MaxValue,
    };
}

/// <summary>A run of links at consecutive addresses sharing an arity range and a reference width.</summary>
public sealed class Section
{
    /// <summary>Addresses skipped before the first link of the section.</summary>
    public ulong Gap { get; set; }

    /// <summary>The number of references each link may hold.</summary>
    public ArityRange Arity { get; set; } = ArityRange.Doublets;

    /// <summary>Bytes per reference: 1, 2, 4 or 8.</summary>
    public byte Width { get; set; } = 1;

    /// <summary>The links, in address order.</summary>
    public List<PacketReference[]> Links { get; } = new();

    internal void WriteHeader(List<byte> output)
    {
        var shape = LinksPacket.WidthCode(Width) | (Arity.Min << LinksPacket.ShapeMinArityShift);
        if (Gap != 0)
        {
            shape |= LinksPacket.ShapeHasGap;
        }
        if (!Arity.IsFixed)
        {
            shape |= LinksPacket.ShapeVariableArity;
        }
        LinksPacket.WriteLeb128(output, shape);
        if (Gap != 0)
        {
            LinksPacket.WriteLeb128(output, Gap);
        }
        if (!Arity.IsFixed)
        {
            LinksPacket.WriteLeb128(output, Arity.Extra);
        }
        LinksPacket.WriteLeb128(output, (ulong)Links.Count);
    }

    /// <summary>Reads a section header, returning the still empty section and its link count.</summary>
    internal static (Section Section, ulong Count) ReadHeader(LinoStreamReader reader)
    {
        var shape = LinksPacket.ReadLeb128(reader);
        var width = LinksPacket.WidthFromCode(shape & LinksPacket.ShapeWidthBits);
        var gap = (shape & LinksPacket.ShapeHasGap) != 0 ? LinksPacket.ReadLeb128(reader) : 0;
        ulong? extra = (shape & LinksPacket.ShapeVariableArity) != 0 ? LinksPacket.ReadLeb128(reader) : null;
        var arity = ArityRange.FromShape(shape >> LinksPacket.ShapeMinArityShift, extra);
        var count = LinksPacket.ReadLeb128(reader);
        return (new Section { Gap = gap, Arity = arity, Width = width }, count);
    }

    internal void Validate()
    {
        LinksPacket.WidthCode(Width);
        if (Arity.Problem() is { } problem)
        {
            throw LinoProtocolException.Unencodable(problem);
        }
        foreach (var link in Links)
        {
            if (!Arity.Contains((ulong)link.Length))
            {
                throw LinoProtocolException.Unencodable(
                    $"a link of {link.Length} references in a section of arity {Arity}");
            }
        }
    }
}

/// <summary>
/// Binary links notation: a self-delimiting packet of links.
/// </summary>
/// <remarks>
/// A packet stores links, each a tuple of one or more references, at
/// implicit consecutive addresses. It knows nothing about LiNo; the LiNo
/// protocol (<see cref="LinoMapping"/>) is one use of it.
/// <code>
/// byte 0     0x10 | flags      high nibble 1 = format version 1
///                              bit 0     external references (Hybrid encoding)
///                              bit 1     explicit layout
///                              bits 2-3  log2 of the width (compact layout only)
///
/// compact layout (bit 1 clear): one section of doublets right after the markers
/// LEB128     N                 number of links, each `source target`
///
/// explicit layout (bit 1 set):
/// LEB128     S                 number of sections, then S section headers:
/// LEB128     shape             bits 0-1  log2 of the reference width in bytes
///                              bit 2     a gap follows
///                              bit 3     variable arity (else every link
///                                        holds exactly min_arity references)
///                              bits 4+   min_arity, at least 1
/// LEB128     gap               addresses skipped before the section (if bit 2)
/// LEB128     extra_arity       0 = no maximum, else max - min (if bit 3)
/// LEB128     count             number of links in the section
///
/// links, section by section; a link in a variable-arity section starts
/// with LEB128 (length - min_arity); every reference is `width` bytes,
/// little-endian
/// </code>
/// Address 0 is null. The first section starts at <c>1 + gap</c> and every
/// other section at <c>previous end + gap</c>, so gaps leave holes. The
/// compact layout is exactly one section with gap 5 (the LiNo marker points
/// 1..5), arity 2 and the header width. Each section has its own width: the
/// narrowest of 1, 2, 4 and 8 bytes that holds every reference in it;
/// <see cref="Pack"/> chooses the sections. With external references
/// enabled the top bit of a reference marks it as external, exactly like
/// <c>Platform.Data.Hybrid&lt;T&gt;</c>, which halves every internal range.
/// The byte layout is identical to the Rust <c>link_cli::protocol::LinksPacket</c>.
/// </remarks>
public sealed class LinksPacket
{
    /// <summary>The high nibble of the header byte; text never starts with 0x10..0x1F.</summary>
    public const byte BinaryVersion1 = 0x10;

    /// <summary>Null link address.</summary>
    public const ulong NullAddress = 0;
    /// <summary>Marker 1: the unary one; 2^k = (2^(k-1) 2^(k-1)).</summary>
    public const ulong One = 1;
    /// <summary>Marker 2: (Number unary) is a non-negative integer.</summary>
    public const ulong Number = 2;
    /// <summary>Marker 3: (String code points…) is a Unicode string.</summary>
    public const ulong String = 3;
    /// <summary>Marker 4: (List elements…) is a list of links.</summary>
    public const ulong List = 4;
    /// <summary>Marker 5: (Identified id values…) is a link with an id.</summary>
    public const ulong Identified = 5;
    /// <summary>Address of the first link after the marker points.</summary>
    public const ulong FirstLinkAddress = 6;

    private const byte FlagExternalReferences = 0b0001;
    private const byte FlagExplicitLayout = 0b0010;
    private const int WidthShift = 2;
    private const byte WidthBits = 0b1100;

    internal const ulong ShapeWidthBits = 0b0011;
    internal const ulong ShapeHasGap = 0b0100;
    internal const ulong ShapeVariableArity = 0b1000;
    internal const int ShapeMinArityShift = 4;

    /// <summary>The addresses the compact layout skips: the marker points.</summary>
    private const ulong CompactGap = FirstLinkAddress - 1;

    private static readonly byte[] WidthValues = { 1, 2, 4, 8 };

    /// <summary>The reference widths, in bytes, that a packet may use.</summary>
    public static IReadOnlyList<byte> Widths => WidthValues;

    /// <summary>An empty packet.</summary>
    public LinksPacket(bool externalReferences = false) => ExternalReferences = externalReferences;

    /// <summary>Header bit 0: references may be external (Hybrid encoding).</summary>
    public bool ExternalReferences { get; set; }

    /// <summary>The links, section by section.</summary>
    public List<Section> Sections { get; } = new();

    /// <summary>Largest internal address that fits in <paramref name="width"/> bytes.</summary>
    public static ulong InternalCapacity(byte width, bool externalReferences)
    {
        var bits = width * 8 - (externalReferences ? 1 : 0);
        return bits >= 64 ? ulong.MaxValue : (1UL << bits) - 1;
    }

    /// <summary>Largest external value that fits in <paramref name="width"/> bytes.</summary>
    public static ulong ExternalCapacity(byte width) => (1UL << (width * 8 - 1)) - 1;

    /// <summary>The narrowest width able to hold the internal address.</summary>
    public static byte AddressTier(ulong address, bool externalReferences)
    {
        foreach (var width in WidthValues)
        {
            if (InternalCapacity(width, externalReferences) >= address)
            {
                return width;
            }
        }
        throw LinoProtocolException.Unencodable($"address {address} exceeds the internal range");
    }

    private static ulong WidthMask(byte width) => InternalCapacity(width, false);

    /// <summary>Encodes an external value at <paramref name="width"/> the way Hybrid&lt;T&gt; does.</summary>
    public static ulong EncodeExternal(ulong value, byte width) =>
        value == 0 ? 1UL << (width * 8 - 1) : (0UL - value) & WidthMask(width);

    /// <summary>Decodes a raw value; returns true and the value for externals.</summary>
    public static bool TryDecodeExternal(ulong raw, byte width, out ulong value)
    {
        var externalZero = 1UL << (width * 8 - 1);
        value = 0;
        if (raw == externalZero)
        {
            return true;
        }
        if (raw > externalZero)
        {
            value = (0UL - raw) & WidthMask(width);
            return true;
        }
        return false;
    }

    internal static ulong WidthCode(byte width)
    {
        var code = Array.IndexOf(WidthValues, width);
        return code >= 0 ? (ulong)code : throw LinoProtocolException.Unencodable($"invalid width {width}");
    }

    internal static byte WidthFromCode(ulong code) =>
        code < (ulong)WidthValues.Length
            ? WidthValues[code]
            : throw LinoProtocolException.Malformed($"invalid width code {code}");

    /// <summary>The narrowest width able to hold <paramref name="reference"/>.</summary>
    public static byte ReferenceWidth(PacketReference reference, bool externalReferences)
    {
        if (!reference.IsExternal)
        {
            return AddressTier(reference.Value, externalReferences);
        }
        if (!externalReferences)
        {
            throw LinoProtocolException.Unencodable("external reference in a packet without external references");
        }
        foreach (var width in WidthValues)
        {
            if (ExternalCapacity(width) >= reference.Value)
            {
                return width;
            }
        }
        throw LinoProtocolException.Unencodable($"external value {reference.Value} exceeds 63 bits");
    }

    /// <summary>
    /// Lays out <paramref name="links"/> — addresses with their references, in
    /// ascending address order — in as few bytes as the format allows.
    /// </summary>
    /// <remarks>
    /// Holes between addresses start new sections. Without
    /// <paramref name="packedWidths"/> every section uses the width of the
    /// widest reference, so all references have the same size. With it each
    /// section gets the narrowest width its links need and sections split
    /// wherever that saves bytes; the result is never larger than the uniform one.
    /// </remarks>
    public static LinksPacket Pack(
        bool externalReferences,
        IReadOnlyList<(ulong Address, PacketReference[] References)> links,
        bool packedWidths)
    {
        ArgumentNullException.ThrowIfNull(links);
        var planner = new SectionPlanner(externalReferences, links);
        var uniformLayout = planner.Plan(false);
        var uniform = LayOut(externalReferences, links, uniformLayout);
        if (!packedWidths)
        {
            return uniform;
        }
        var packedLayout = planner.Plan(true);
        if (packedLayout.SequenceEqual(uniformLayout))
        {
            return uniform;
        }
        var packed = LayOut(externalReferences, links, packedLayout);
        return packed.ToBytes().Length < uniform.ToBytes().Length ? packed : uniform;
    }

    /// <summary>Splits <paramref name="links"/> into sections of <c>(link count, width)</c>.</summary>
    private static LinksPacket LayOut(
        bool externalReferences,
        IReadOnlyList<(ulong Address, PacketReference[] References)> links,
        IReadOnlyList<(int Count, byte Width)> layout)
    {
        var packet = new LinksPacket(externalReferences);
        var nextAddress = 1UL;
        var first = 0;
        foreach (var (count, width) in layout)
        {
            var start = links[first].Address;
            var shortest = ulong.MaxValue;
            var longest = 0UL;
            var section = new Section { Gap = start - nextAddress, Width = width };
            for (var index = first; index < first + count; index++)
            {
                var length = (ulong)links[index].References.Length;
                shortest = Math.Min(shortest, length);
                longest = Math.Max(longest, length);
                section.Links.Add(links[index].References);
            }
            section.Arity = ArityRange.Between(shortest, longest);
            packet.Sections.Add(section);
            nextAddress = start + (ulong)count;
            first += count;
        }
        return packet;
    }

    /// <summary>Every link with its address, in address order.</summary>
    public IEnumerable<(ulong Address, PacketReference[] References)> Links()
    {
        var nextAddress = 1UL;
        foreach (var section in Sections)
        {
            var start = SaturatingAdd(nextAddress, section.Gap);
            nextAddress = SaturatingAdd(start, (ulong)section.Links.Count);
            for (var index = 0; index < section.Links.Count; index++)
            {
                yield return (start + (ulong)index, section.Links[index]);
            }
        }
    }

    private static ulong SaturatingAdd(ulong left, ulong right) =>
        left > ulong.MaxValue - right ? ulong.MaxValue : left + right;

    /// <summary>The number of links in the packet.</summary>
    public ulong LinkCount => Sections.Aggregate(0UL, (total, section) => total + (ulong)section.Links.Count);

    private bool IsCompact => Sections.Count switch
    {
        0 => true,
        1 => Sections[0].Gap == CompactGap && Sections[0].Arity == ArityRange.Doublets && Sections[0].Links.Count > 0,
        _ => false,
    };

    /// <summary>Serializes the packet.</summary>
    public byte[] ToBytes()
    {
        var header = BinaryVersion1;
        if (ExternalReferences)
        {
            header |= FlagExternalReferences;
        }
        var nextAddress = 1UL;
        foreach (var section in Sections)
        {
            section.Validate();
            var count = (ulong)section.Links.Count;
            if (section.Gap > ulong.MaxValue - nextAddress || count > ulong.MaxValue - nextAddress - section.Gap)
            {
                throw LinoProtocolException.Unencodable("addresses overflow 64 bits");
            }
            nextAddress += section.Gap + count;
        }
        var output = new List<byte>();
        if (IsCompact)
        {
            var width = Sections.Count > 0 ? Sections[0].Width : (byte)1;
            output.Add((byte)(header | (WidthCode(width) << WidthShift)));
            WriteLeb128(output, LinkCount);
        }
        else
        {
            output.Add((byte)(header | FlagExplicitLayout));
            WriteLeb128(output, (ulong)Sections.Count);
            foreach (var section in Sections)
            {
                section.WriteHeader(output);
            }
        }
        foreach (var section in Sections)
        {
            foreach (var link in section.Links)
            {
                if (!section.Arity.IsFixed)
                {
                    WriteLeb128(output, (ulong)link.Length - section.Arity.Min);
                }
                foreach (var reference in link)
                {
                    WriteRaw(output, RawReference(reference, section.Width), section.Width);
                }
            }
        }
        return output.ToArray();
    }

    private ulong RawReference(PacketReference reference, byte width)
    {
        if (ReferenceWidth(reference, ExternalReferences) > width)
        {
            throw LinoProtocolException.Unencodable($"{reference} does not fit {width} byte(s)");
        }
        return reference.IsExternal ? EncodeExternal(reference.Value, width) : reference.Value;
    }

    /// <summary>Parses a complete packet; trailing bytes are an error.</summary>
    public static LinksPacket FromBytes(byte[] bytes, DecodeLimits? limits = null)
    {
        var reader = LinoStreamReader.FromBytes(bytes);
        var packet = ReadFrom(reader, limits) ?? throw LinoProtocolException.Malformed("empty input");
        if (!reader.AtEnd)
        {
            throw LinoProtocolException.Malformed("trailing bytes after the packet");
        }
        return packet;
    }

    /// <summary>Reads one packet; null on a clean end of stream.</summary>
    public static LinksPacket? ReadFrom(LinoStreamReader reader, DecodeLimits? limits = null)
    {
        ArgumentNullException.ThrowIfNull(reader);
        limits ??= DecodeLimits.Default;
        var first = reader.ReadByte();
        if (first < 0)
        {
            return null;
        }
        var header = (byte)first;
        if ((header & 0xF0) != BinaryVersion1)
        {
            throw LinoProtocolException.Malformed($"unsupported binary header byte 0x{header:X2}");
        }
        var packet = new LinksPacket((header & FlagExternalReferences) != 0);
        var counts = new List<ulong>();
        if ((header & FlagExplicitLayout) == 0)
        {
            var width = WidthFromCode((ulong)((header & WidthBits) >> WidthShift));
            var count = ReadLeb128(reader);
            if (count > 0)
            {
                packet.Sections.Add(new Section { Gap = CompactGap, Arity = ArityRange.Doublets, Width = width });
                counts.Add(count);
            }
        }
        else
        {
            if ((header & WidthBits) != 0)
            {
                throw LinoProtocolException.Malformed("the explicit layout keeps the header width bits clear");
            }
            var sectionCount = ReadLeb128(reader);
            if (sectionCount > limits.MaxLinks)
            {
                throw TooManyLinks(limits);
            }
            var nextAddress = 1UL;
            for (ulong index = 0; index < sectionCount; index++)
            {
                var (section, count) = Section.ReadHeader(reader);
                if (section.Gap > ulong.MaxValue - nextAddress || count > ulong.MaxValue - nextAddress - section.Gap)
                {
                    throw LinoProtocolException.Malformed("addresses overflow 64 bits");
                }
                nextAddress += section.Gap + count;
                packet.Sections.Add(section);
                counts.Add(count);
            }
        }
        var total = 0UL;
        foreach (var count in counts)
        {
            if (count > limits.MaxLinks - total)
            {
                throw TooManyLinks(limits);
            }
            total += count;
        }
        var referencesLeft = limits.MaxReferences;
        for (var index = 0; index < packet.Sections.Count; index++)
        {
            var section = packet.Sections[index];
            section.Links.Capacity = (int)Math.Min(counts[index], 4096);
            for (ulong link = 0; link < counts[index]; link++)
            {
                var length = section.Arity.Min;
                if (!section.Arity.IsFixed)
                {
                    var extra = ReadLeb128(reader);
                    if (extra > ulong.MaxValue - length || !section.Arity.Contains(extra + length))
                    {
                        throw LinoProtocolException.Malformed($"link length outside the section arity {section.Arity}");
                    }
                    length += extra;
                }
                if (length > referencesLeft)
                {
                    throw LinoProtocolException.Limit($"references exceed the limit of {limits.MaxReferences}");
                }
                referencesLeft -= length;
                var references = new List<PacketReference>((int)Math.Min(length, 4096));
                for (ulong item = 0; item < length; item++)
                {
                    var raw = ReadRaw(reader, section.Width);
                    references.Add(packet.ExternalReferences && TryDecodeExternal(raw, section.Width, out var value)
                        ? PacketReference.External(value)
                        : PacketReference.Internal(raw));
                }
                section.Links.Add(references.ToArray());
            }
        }
        return packet;
    }

    private static LinoProtocolException TooManyLinks(DecodeLimits limits) =>
        LinoProtocolException.Limit($"packet declares more than {limits.MaxLinks} links");

    private static void WriteRaw(List<byte> output, ulong value, byte width)
    {
        for (var index = 0; index < width; index++)
        {
            output.Add((byte)(value >> (8 * index)));
        }
    }

    private static ulong ReadRaw(LinoStreamReader reader, byte width)
    {
        Span<byte> bytes = stackalloc byte[8];
        bytes.Clear();
        reader.ReadExactly(bytes[..width]);
        return BinaryPrimitives.ReadUInt64LittleEndian(bytes);
    }

    /// <summary>Appends <paramref name="value"/> as unsigned LEB128.</summary>
    public static void WriteLeb128(List<byte> output, ulong value)
    {
        ArgumentNullException.ThrowIfNull(output);
        while (true)
        {
            var current = (byte)(value & 0x7F);
            value >>= 7;
            if (value == 0)
            {
                output.Add(current);
                return;
            }
            output.Add((byte)(current | 0x80));
        }
    }

    /// <summary>Reads an unsigned LEB128 value of at most 64 bits.</summary>
    public static ulong ReadLeb128(LinoStreamReader reader)
    {
        ArgumentNullException.ThrowIfNull(reader);
        ulong value = 0;
        for (var shift = 0; shift < 64; shift += 7)
        {
            var next = reader.ReadByte();
            if (next < 0)
            {
                throw LinoProtocolException.Malformed("unexpected end of packet");
            }
            var payload = (ulong)(next & 0x7F);
            if (shift == 63 && payload > 1)
            {
                throw LinoProtocolException.Malformed("LEB128 value overflows 64 bits");
            }
            value |= payload << shift;
            if ((next & 0x80) == 0)
            {
                return value;
            }
        }
        throw LinoProtocolException.Malformed("LEB128 value overflows 64 bits");
    }
}
