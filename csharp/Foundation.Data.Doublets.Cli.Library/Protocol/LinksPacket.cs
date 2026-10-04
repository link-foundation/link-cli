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
    /// <summary>Maximum N + M.</summary>
    public ulong MaxLinks { get; init; } = 1UL << 22;

    /// <summary>Maximum total number of references inside all sequences.</summary>
    public ulong MaxSequenceItems { get; init; } = 1UL << 24;

    /// <summary>Maximum number of LiNo nodes a packet may expand to.</summary>
    public long MaxNodes { get; init; } = 1L << 22;

    /// <summary>Maximum LiNo nesting depth.</summary>
    public int MaxDepth { get; init; } = 1024;

    /// <summary>Maximum size of a text message in bytes.</summary>
    public long MaxTextBytes { get; init; } = 64L << 20;

    /// <summary>The default limits.</summary>
    public static DecodeLimits Default { get; } = new();
}

/// <summary>
/// The binary links packet: the wire format of the binary LiNo protocol.
/// </summary>
/// <remarks>
/// <code>
/// byte 0      0x10 | flags        high nibble 1 = format version 1
///                                 bit 0  external references (Hybrid encoding)
///                                 bit 1  sequence section present
///                                 bits 2-3  log2 of the minimum width in bytes
/// LEB128      N                   number of fixed doublets
/// LEB128      M                   number of sequences (only when bit 1 is set)
/// N times     source target       one fixed doublet, refs width(a) bytes each
/// M times     size ref_1 … ref_n  one sequence, size and refs width(a) bytes each
/// </code>
/// Addresses are implicit: 0 is null, 1..5 are the reserved marker points
/// (never transmitted), the fixed doublets occupy 6..6+N and the sequences
/// follow them. Every reference of the link at address a uses
/// width(a) = max(min_width, tier(a)) bytes, little-endian, where tier(a) is
/// the smallest of 1, 2, 4 and 8 bytes able to hold a. With external
/// references enabled the top bit marks a reference as external, exactly like
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
    /// <summary>Address of the first transmitted link.</summary>
    public const ulong FirstLinkAddress = 6;

    private const byte FlagExternalReferences = 0b0001;
    private const byte FlagSequences = 0b0010;
    private const int WidthShift = 2;

    /// <summary>The reference widths, in bytes, that a packet may use.</summary>
    public static IReadOnlyList<byte> Widths { get; } = new byte[] { 1, 2, 4, 8 };

    /// <summary>Header bit 0: references may be external (Hybrid encoding).</summary>
    public bool ExternalReferences { get; set; }

    /// <summary>Header bit 1: the packet carries a sequence section (possibly empty).</summary>
    public bool SequencesSection { get; set; }

    /// <summary>Minimum reference width in bytes: 1, 2, 4 or 8.</summary>
    public byte MinWidth { get; set; } = 1;

    /// <summary>Fixed doublets at addresses 6..6+N.</summary>
    public List<(PacketReference Source, PacketReference Target)> Doublets { get; } = new();

    /// <summary>Variable-length sequences at addresses 6+N..6+N+M.</summary>
    public List<List<PacketReference>> Sequences { get; } = new();

    /// <summary>Largest internal address that fits in <paramref name="width"/> bytes.</summary>
    public static ulong InternalCapacity(byte width, bool externalReferences)
    {
        var bits = width * 8 - (externalReferences ? 1 : 0);
        return bits >= 64 ? ulong.MaxValue : (1UL << bits) - 1;
    }

    /// <summary>Largest external value that fits in <paramref name="width"/> bytes.</summary>
    public static ulong ExternalCapacity(byte width) => (1UL << (width * 8 - 1)) - 1;

    /// <summary>Largest unsigned value (a sequence size) that fits in <paramref name="width"/> bytes.</summary>
    public static ulong UnsignedCapacity(byte width) => InternalCapacity(width, false);

    /// <summary>The narrowest width able to hold the internal address.</summary>
    public static byte AddressTier(ulong address, bool externalReferences)
    {
        foreach (var width in Widths)
        {
            if (InternalCapacity(width, externalReferences) >= address)
            {
                return width;
            }
        }
        return 8;
    }

    /// <summary>Encodes an external value at <paramref name="width"/> the way Hybrid&lt;T&gt; does.</summary>
    public static ulong EncodeExternal(ulong value, byte width) =>
        value == 0 ? 1UL << (width * 8 - 1) : (0UL - value) & UnsignedCapacity(width);

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
            value = (0UL - raw) & UnsignedCapacity(width);
            return true;
        }
        return false;
    }

    /// <summary>Address of the fixed doublet with zero-based index.</summary>
    public ulong DoubletAddress(int index) => FirstLinkAddress + (ulong)index;

    /// <summary>Address of the sequence with zero-based index.</summary>
    public ulong SequenceAddress(int index) => FirstLinkAddress + (ulong)Doublets.Count + (ulong)index;

    /// <summary>The highest address in the packet, or null for an empty packet.</summary>
    public ulong? LastAddress
    {
        get
        {
            var count = Doublets.Count + Sequences.Count;
            return count > 0 ? FirstLinkAddress + (ulong)count - 1 : null;
        }
    }

    /// <summary>Width used by every reference of the link at <paramref name="address"/>.</summary>
    public byte WidthAt(ulong address) => Math.Max(MinWidth, AddressTier(address, ExternalReferences));

    /// <summary>
    /// The smallest min_width able to encode the packet; with <paramref name="uniform"/>
    /// at least the tier of the last address, so every reference has the same width.
    /// </summary>
    public byte RequiredMinWidth(bool uniform)
    {
        byte needed = 1;
        void Note(ulong address, byte need)
        {
            if (need > AddressTier(address, ExternalReferences))
            {
                needed = Math.Max(needed, need);
            }
        }
        for (var index = 0; index < Doublets.Count; index++)
        {
            var (source, target) = Doublets[index];
            Note(DoubletAddress(index), Math.Max(ReferenceNeed(source), ReferenceNeed(target)));
        }
        for (var index = 0; index < Sequences.Count; index++)
        {
            var items = Sequences[index];
            var need = Widths.FirstOrDefault(width => UnsignedCapacity(width) >= (ulong)items.Count, (byte)8);
            foreach (var item in items)
            {
                need = Math.Max(need, ReferenceNeed(item));
            }
            Note(SequenceAddress(index), need);
        }
        if (uniform && LastAddress is { } last)
        {
            needed = Math.Max(needed, AddressTier(last, ExternalReferences));
        }
        return needed;
    }

    private byte ReferenceNeed(PacketReference reference)
    {
        if (!reference.IsExternal)
        {
            return 1;
        }
        if (!ExternalReferences)
        {
            throw LinoProtocolException.Unencodable("external reference in a packet without external references");
        }
        foreach (var width in Widths)
        {
            if (ExternalCapacity(width) >= reference.Value)
            {
                return width;
            }
        }
        throw LinoProtocolException.Unencodable($"external value {reference.Value} exceeds 63 bits");
    }

    private byte HeaderByte()
    {
        var code = Widths.ToList().IndexOf(MinWidth);
        if (code < 0)
        {
            throw LinoProtocolException.Unencodable($"invalid width {MinWidth}");
        }
        var header = (byte)(BinaryVersion1 | (code << WidthShift));
        if (ExternalReferences)
        {
            header |= FlagExternalReferences;
        }
        if (SequencesSection)
        {
            header |= FlagSequences;
        }
        return header;
    }

    private ulong RawReference(PacketReference reference, ulong address, byte width)
    {
        if (!reference.IsExternal)
        {
            if (reference.Value >= address)
            {
                throw LinoProtocolException.Unencodable($"link {address} refers forward to {reference.Value}");
            }
            return reference.Value;
        }
        if (!ExternalReferences || reference.Value > ExternalCapacity(width))
        {
            throw LinoProtocolException.Unencodable(
                $"external value {reference.Value} does not fit {width} byte(s) at link {address}");
        }
        return EncodeExternal(reference.Value, width);
    }

    /// <summary>Serializes the packet.</summary>
    public byte[] ToBytes()
    {
        if (!SequencesSection && Sequences.Count > 0)
        {
            throw LinoProtocolException.Unencodable("sequences in a packet without a sequence section");
        }
        var output = new List<byte> { HeaderByte() };
        WriteLeb128(output, (ulong)Doublets.Count);
        if (SequencesSection)
        {
            WriteLeb128(output, (ulong)Sequences.Count);
        }
        for (var index = 0; index < Doublets.Count; index++)
        {
            var address = DoubletAddress(index);
            var width = WidthAt(address);
            var (source, target) = Doublets[index];
            WriteRaw(output, RawReference(source, address, width), width);
            WriteRaw(output, RawReference(target, address, width), width);
        }
        for (var index = 0; index < Sequences.Count; index++)
        {
            var address = SequenceAddress(index);
            var width = WidthAt(address);
            var items = Sequences[index];
            var size = (ulong)items.Count;
            if (size > UnsignedCapacity(width))
            {
                throw LinoProtocolException.Unencodable(
                    $"sequence size {size} does not fit {width} byte(s) at link {address}");
            }
            WriteRaw(output, size, width);
            foreach (var item in items)
            {
                WriteRaw(output, RawReference(item, address, width), width);
            }
        }
        return output.ToArray();
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
        var packet = new LinksPacket
        {
            ExternalReferences = (header & FlagExternalReferences) != 0,
            SequencesSection = (header & FlagSequences) != 0,
            MinWidth = Widths[(header >> WidthShift) & 0b11],
        };
        var doubletCount = ReadLeb128(reader);
        var sequenceCount = packet.SequencesSection ? ReadLeb128(reader) : 0;
        if (doubletCount > limits.MaxLinks || sequenceCount > limits.MaxLinks - doubletCount)
        {
            throw LinoProtocolException.Limit(
                $"packet declares {doubletCount} + {sequenceCount} links, limit is {limits.MaxLinks}");
        }
        for (var index = 0; index < (int)doubletCount; index++)
        {
            var address = packet.DoubletAddress(index);
            var width = packet.WidthAt(address);
            var source = packet.ReadReference(reader, address, width);
            var target = packet.ReadReference(reader, address, width);
            packet.Doublets.Add((source, target));
        }
        var itemsLeft = limits.MaxSequenceItems;
        for (var index = 0; index < (int)sequenceCount; index++)
        {
            var address = packet.SequenceAddress(index);
            var width = packet.WidthAt(address);
            var size = ReadRaw(reader, width);
            if (size > itemsLeft)
            {
                throw LinoProtocolException.Limit($"sequence items exceed the limit of {limits.MaxSequenceItems}");
            }
            itemsLeft -= size;
            var items = new List<PacketReference>((int)Math.Min(size, 4096));
            for (ulong item = 0; item < size; item++)
            {
                items.Add(packet.ReadReference(reader, address, width));
            }
            packet.Sequences.Add(items);
        }
        return packet;
    }

    private PacketReference ReadReference(LinoStreamReader reader, ulong address, byte width)
    {
        var raw = ReadRaw(reader, width);
        if (ExternalReferences && TryDecodeExternal(raw, width, out var value))
        {
            return PacketReference.External(value);
        }
        if (raw >= address)
        {
            throw LinoProtocolException.Malformed($"link {address} refers to {raw}, which is not an earlier link");
        }
        return PacketReference.Internal(raw);
    }

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
