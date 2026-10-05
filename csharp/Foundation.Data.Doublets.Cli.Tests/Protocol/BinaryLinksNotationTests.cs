using Foundation.Data.Doublets.Cli.Protocol;
using Platform.Data;

using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

/// <summary>
/// The binary links notation: packets, sections, arities, widths and the
/// LiNo mapping on top of them. Mirrors <c>rust/tests/protocol_packet_tests.rs</c>.
/// </summary>
public sealed class BinaryLinksNotationTests
{
    internal static string Hex(byte[] bytes) => string.Join(" ", bytes.Select(value => value.ToString("x2")));

    internal static byte[] Unhex(string text) =>
        text.Split(' ', StringSplitOptions.RemoveEmptyEntries).Select(value => Convert.ToByte(value, 16)).ToArray();

    private static IReadOnlyList<LinoLink> Parse(string text) => LinoFormat.ParseDocument(text);

    private static bool ReferencesOption(string field) => field switch
    {
        "plain" => false,
        "external" => true,
        _ => throw new ArgumentException($"unknown references option {field}"),
    };

    private static bool WidthsOption(string field) => field switch
    {
        "uniform" => false,
        "packed" => true,
        _ => throw new ArgumentException($"unknown widths option {field}"),
    };

    /// <summary><c>address:reference reference…</c> separated by <c>;</c>, <c>#v</c> being external.</summary>
    private static List<(ulong Address, PacketReference[] References)> ParseLinks(string text) =>
        text.Split(';').Select(link =>
        {
            var parts = link.Split(':', 2);
            var references = parts[1].Split(' ').Select(reference => reference.StartsWith('#')
                ? PacketReference.External(ulong.Parse(reference[1..]))
                : PacketReference.Internal(ulong.Parse(reference))).ToArray();
            return (ulong.Parse(parts[0]), references);
        }).ToList();

    /// <summary>The golden vectors shared with the Rust test suite: both ports must write and read exactly these bytes.</summary>
    private static IEnumerable<string[]> GoldenVectors() =>
        File.ReadAllLines(Path.Combine(AppContext.BaseDirectory, "Protocol", "binary-links-notation-vectors.txt"))
            .Where(line => line.Length > 0 && !line.StartsWith('#'))
            .Select(line => line.Split('\t'));

    private static string Describe(IEnumerable<(ulong Address, PacketReference[] References)> links) =>
        string.Join(";", links.Select(link => $"{link.Address}:{string.Join(" ", link.References)}"));

    private static string Describe(LinksPacket packet) =>
        $"{packet.ExternalReferences} " + string.Join(" | ", packet.Sections.Select(section =>
            $"gap {section.Gap} arity {section.Arity} width {section.Width}: "
            + string.Join(";", section.Links.Select(link => string.Join(" ", link)))));

    [Fact]
    public void GoldenDocumentsAreStable()
    {
        var checkedVectors = 0;
        foreach (var fields in GoldenVectors().Where(fields => fields[0] == "document"))
        {
            Assert.Equal(6, fields.Length);
            var text = fields[1].Replace("\\n", "\n");
            var options = new BinaryLinoOptions
            {
                ExternalReferences = ReferencesOption(fields[2]),
                Arity = ArityRange.Parse(fields[3]),
                PackedWidths = WidthsOption(fields[4]),
            };
            var document = Parse(text);
            var binary = new BinaryLinoProtocol(options);
            Assert.True(fields[5] == Hex(binary.Encode(document)), $"{text} with {options}");
            Assert.Equal(document, binary.Decode(Unhex(fields[5])));
            checkedVectors++;
        }
        Assert.True(checkedVectors >= 80, $"only {checkedVectors} document vectors");
    }

    [Fact]
    public void GoldenPacketsAreStable()
    {
        var checkedVectors = 0;
        foreach (var fields in GoldenVectors().Where(fields => fields[0] == "links"))
        {
            Assert.Equal(5, fields.Length);
            var links = ParseLinks(fields[1]);
            var packet = LinksPacket.Pack(ReferencesOption(fields[2]), links, WidthsOption(fields[3]));
            Assert.True(fields[4] == Hex(packet.ToBytes()), fields[1]);
            var read = LinksPacket.FromBytes(Unhex(fields[4]));
            Assert.Equal(Describe(packet), Describe(read));
            Assert.Equal(Describe(links), Describe(read.Links()));
            checkedVectors++;
        }
        Assert.True(checkedVectors >= 10, $"only {checkedVectors} links vectors");
    }

    [Fact]
    public void WidthTiersFollowTheNumberOfLinks()
    {
        Assert.Equal(1, LinksPacket.AddressTier(0, false));
        Assert.Equal(1, LinksPacket.AddressTier(255, false));
        Assert.Equal(2, LinksPacket.AddressTier(256, false));
        Assert.Equal(2, LinksPacket.AddressTier(65_535, false));
        Assert.Equal(4, LinksPacket.AddressTier(65_536, false));
        Assert.Equal(4, LinksPacket.AddressTier(uint.MaxValue, false));
        Assert.Equal(8, LinksPacket.AddressTier((ulong)uint.MaxValue + 1, false));
        // External references take the top bit, halving every range.
        Assert.Equal(1, LinksPacket.AddressTier(127, true));
        Assert.Equal(2, LinksPacket.AddressTier(128, true));
        Assert.Equal(2, LinksPacket.AddressTier(32_767, true));
        Assert.Equal(4, LinksPacket.AddressTier(32_768, true));
        Assert.Equal(8, LinksPacket.AddressTier(ulong.MaxValue, false));
        // No width holds an internal address in the external half.
        Assert.Equal(LinoProtocolErrorKind.Unencodable, Assert.Throws<LinoProtocolException>(() => LinksPacket.AddressTier(1UL << 63, true)).Kind);
        Assert.Equal(ulong.MaxValue, LinksPacket.InternalCapacity(8, false));
        Assert.Equal((ulong)long.MaxValue, LinksPacket.InternalCapacity(8, true));
        Assert.Equal(127UL, LinksPacket.ExternalCapacity(1));
    }

    [Fact]
    public void ExternalReferencesMatchPlatformDataHybrid()
    {
        foreach (var width in LinksPacket.Widths)
        {
            foreach (var value in new ulong[] { 0, 1, 2, 100, LinksPacket.ExternalCapacity(width) })
            {
                var raw = LinksPacket.EncodeExternal(value, width);
                Assert.True(LinksPacket.TryDecodeExternal(raw, width, out var decoded));
                Assert.Equal(value, decoded);
            }
            Assert.False(LinksPacket.TryDecodeExternal(LinksPacket.InternalCapacity(width, true), width, out _));
        }
        Assert.Equal(0xFFUL, LinksPacket.EncodeExternal(1, 1));
        Assert.Equal(0x80UL, LinksPacket.EncodeExternal(0, 1));
        foreach (var value in new uint[] { 0, 1, 5, 1000, int.MaxValue })
        {
            Assert.Equal((ulong)new Hybrid<uint>(value, isExternal: true).Value, LinksPacket.EncodeExternal(value, 4));
        }
    }

    private static string ManyLinks(int count) =>
        string.Join("\n", Enumerable.Range(0, count).Select(index => $"(n{index} m{index})"));

    private static byte[] SectionWidths(LinksPacket packet) => packet.Sections.Select(section => section.Width).ToArray();

    [Fact]
    public void UniformWidthsGrowPast256AddressesAndPackedWidthsStaySmall()
    {
        Assert.Equal(new byte[] { 1 }, SectionWidths(LinoMapping.EncodeDocument(Parse(ManyLinks(3)))));

        var document = Parse(ManyLinks(400));
        var uniform = LinoMapping.EncodeDocument(document);
        Assert.Equal(new byte[] { 2 }, SectionWidths(uniform));
        var uniformBytes = uniform.ToBytes();
        const int header = 1 + 2; // header byte + two-byte LEB128 count
        Assert.Equal(header + (long)uniform.LinkCount * 2 * 2, uniformBytes.Length);

        var packed = LinoMapping.EncodeDocument(document, new BinaryLinoOptions().WithPackedWidths());
        Assert.Equal(1, SectionWidths(packed)[0]);
        Assert.Contains((byte)2, SectionWidths(packed));
        var packedBytes = packed.ToBytes();
        Assert.True(packedBytes.Length < uniformBytes.Length);
        foreach (var bytes in new[] { uniformBytes, packedBytes })
        {
            Assert.Equal(document, LinoMapping.DecodeDocument(LinksPacket.FromBytes(bytes)));
        }
    }

    [Fact]
    public void PackedWidthsNeverTakeMoreBytesThanUniformOnes()
    {
        var corpus = LinoProtocolCodecTests.Corpus.Append(ManyLinks(200)).Append("(1000 1)");
        foreach (var text in corpus)
        {
            var document = Parse(text);
            foreach (var options in LinoProtocolCodecTests.AllOptions().Where(options => options.PackedWidths))
            {
                var packed = LinoMapping.EncodeDocument(document, options).ToBytes();
                var uniform = LinoMapping.EncodeDocument(document, options.WithPackedWidths(false)).ToBytes();
                Assert.True(packed.Length <= uniform.Length, $"{text} with {options}");
            }
        }
    }

    [Fact]
    public void LargeExternalValuesWidenOnlyTheirSection()
    {
        var options = new BinaryLinoOptions().WithExternalReferences().WithPackedWidths();
        Assert.Equal(new byte[] { 1 }, SectionWidths(LinoMapping.EncodeDocument(Parse("(100 1)"), options)));
        Assert.Equal(new byte[] { 4, 1 }, SectionWidths(LinoMapping.EncodeDocument(Parse("(70000 1)"), options)));
        // Beyond 63 bits the number falls back to in-band unary links.
        Assert.True(LinoMapping.EncodeDocument(Parse("18446744073709551615"), options).LinkCount > 60);
    }

    private static int[] LinkLengths(IReadOnlyList<LinoLink> document, BinaryLinoOptions options)
    {
        var packet = LinoMapping.EncodeDocument(document, options);
        Assert.Equal(document, new BinaryLinoProtocol().Decode(packet.ToBytes()));
        return packet.Links().Select(link => link.References.Length).ToArray();
    }

    [Fact]
    public void ArityOptionsChooseDoubletsTripletsOrAnyLength()
    {
        var document = Parse("(a b c)\n(d e)");
        var external = new BinaryLinoOptions().WithExternalReferences();
        Assert.All(LinkLengths(document, external), length => Assert.Equal(2, length));
        // `(a b c)` is one triplet; the strings `(String code point)` stay doublets.
        var triplets = LinkLengths(document, external.WithArity(ArityRange.Between(2, 3)));
        Assert.Single(triplets, length => length == 3);
        Assert.All(triplets, length => Assert.InRange(length, 2, 3));
        Assert.Equal(triplets, LinkLengths(document, external.WithArity(ArityRange.AtLeast(1))));
        // Only any length turns a one-item list, here the root, into one link.
        Assert.Equal(new[] { 2, 2, 2, 2 }, LinkLengths(Parse("a"), external));
        Assert.Equal(new[] { 2, 2, 1 }, LinkLengths(Parse("a"), external.WithArity(ArityRange.AtLeast(1))));
        // Long lists only become single links when the range allows their length.
        var longList = Parse("(a b c d e f g h)");
        foreach (var (arity, singleLink) in new[]
        {
            (ArityRange.Between(2, 3), false),
            (ArityRange.Between(2, 8), true),
            (ArityRange.AtLeast(2), true),
        })
        {
            Assert.True(LinkLengths(longList, external.WithArity(arity)).Max() == 8 == singleLink, arity.ToString());
        }
    }

    [Fact]
    public void AritiesWithoutDoubletsAreUnencodable()
    {
        foreach (var arity in new[] { ArityRange.Exactly(3), ArityRange.Exactly(1), ArityRange.AtLeast(3) })
        {
            var error = Assert.Throws<LinoProtocolException>(
                () => LinoMapping.EncodeDocument(Parse("(a b)"), new BinaryLinoOptions().WithArity(arity)));
            Assert.Equal(LinoProtocolErrorKind.Unencodable, error.Kind);
        }
    }

    [Fact]
    public void ArityRangesParseAndDisplay()
    {
        foreach (var (text, range) in new[]
        {
            ("2", ArityRange.Doublets),
            ("2..3", ArityRange.Between(2, 3)),
            ("1..", ArityRange.AtLeast(1)),
            ("3..3", ArityRange.Exactly(3)),
            ("+2", ArityRange.Doublets),
        })
        {
            Assert.Equal(range, ArityRange.Parse(text));
        }
        Assert.Equal("3", ArityRange.Between(3, 3).ToString());
        Assert.Equal("2..3", ArityRange.Between(2, 3).ToString());
        Assert.Equal("1..", ArityRange.AtLeast(1).ToString());
        Assert.Equal(ArityRange.Doublets, new BinaryLinoOptions().Arity);
        Assert.Equal(default, new BinaryLinoOptions().WithArity(ArityRange.Doublets));
        foreach (var invalid in new[] { "", "0", "0..2", "3..2", "a", "1..b", "..3", "-1", " 2", "2 ", "++2", null })
        {
            Assert.False(ArityRange.TryParse(invalid, out _), invalid);
        }
        Assert.False(ArityRange.TryParse("3..2", out _, out var error));
        Assert.Equal("arity range 3..2 is empty", error);
        Assert.False(ArityRange.TryParse("x", out _, out error));
        Assert.Equal("invalid arity 'x': expected n, min..max or min..", error);
        Assert.Equal("arity must be at least 1", Assert.Throws<FormatException>(() => ArityRange.Parse("0")).Message);
        Assert.True(ArityRange.AtLeast(2).Contains(1_000));
        Assert.False(ArityRange.Between(2, 3).Contains(4));
        Assert.False(ArityRange.Between(2, 3).Contains(1));
        Assert.True(ArityRange.Exactly(2).IsFixed && !ArityRange.AtLeast(2).IsFixed);
    }

    [Fact]
    public void ReplyOptionsFollowTheReceivedPacket()
    {
        var document = Parse("(a b c) 70000");
        var options = new BinaryLinoOptions()
            .WithExternalReferences()
            .WithArity(ArityRange.Between(2, 3))
            .WithPackedWidths();
        Assert.Equal(options, BinaryLinoOptions.OfPacket(LinoMapping.EncodeDocument(document, options)));
        Assert.Equal(default, BinaryLinoOptions.OfPacket(LinoMapping.EncodeDocument(document)));
        Assert.Equal(default, BinaryLinoOptions.OfPacket(new LinksPacket()));
        Assert.Equal("BinaryLinoOptions { ExternalReferences = True, Arity = 2..3, PackedWidths = True }", options.ToString());
    }

    private static Section DoubletSection(ulong gap, ArityRange arity, byte width, params (ulong Source, ulong Target)[] links)
    {
        var section = new Section { Gap = gap, Arity = arity, Width = width };
        section.Links.AddRange(links.Select(link =>
            new[] { PacketReference.Internal(link.Source), PacketReference.Internal(link.Target) }));
        return section;
    }

    [Fact]
    public void HandBuiltPacketsDecode()
    {
        // One doublet (One One) is 2^1 in unary, so `(Number 7)` is the number 2.
        var links = new (ulong, ulong)[] { (1, 1), (2, 6), (7, 0), (4, 8) };
        var compact = new LinksPacket();
        compact.Sections.Add(DoubletSection(5, ArityRange.Doublets, 1, links));
        var bytes = compact.ToBytes();
        Assert.Equal("10 04 01 01 02 06 07 00 04 08", Hex(bytes));
        Assert.Equal("2", LinoFormat.FormatDocument(new BinaryLinoProtocol().Decode(bytes)));

        // The same links as a variable section of width 2 use the explicit layout.
        var explicitLayout = new LinksPacket();
        explicitLayout.Sections.Add(DoubletSection(5, ArityRange.AtLeast(1), 2, links));
        bytes = explicitLayout.ToBytes();
        Assert.Equal("12 01 1d 05 00 04 01 01 00 01 00 01 02 00 06 00 01 07 00 00 00 01 04 00 08 00", Hex(bytes));
        Assert.Equal("2", LinoFormat.FormatDocument(new BinaryLinoProtocol().Decode(bytes)));
        Assert.Equal("10 00", Hex(new LinksPacket().ToBytes()));
    }

    private static PacketReference[] Internals(params ulong[] addresses) =>
        addresses.Select(PacketReference.Internal).ToArray();

    [Fact]
    public void PackingRejectsLinksItCannotLayOut()
    {
        void Unencodable(bool externalReferences, params (ulong, PacketReference[])[] links)
        {
            var error = Assert.Throws<LinoProtocolException>(() => LinksPacket.Pack(externalReferences, links, true));
            Assert.Equal(LinoProtocolErrorKind.Unencodable, error.Kind);
        }
        Unencodable(false, (0, Internals(1))); // address 0 is null
        Unencodable(false, (2, Internals(1)), (1, Internals(1))); // descending
        Unencodable(false, (2, Internals(1)), (2, Internals(1))); // repeated
        Unencodable(false, (1, Array.Empty<PacketReference>())); // no references
        Unencodable(false, (1, new[] { PacketReference.External(1) }));
        Unencodable(true, (1, new[] { PacketReference.External(1UL << 63) }));
        Unencodable(true, (1, Internals(1UL << 63))); // the top bit marks externals
        Assert.NotNull(LinksPacket.Pack(false, new[] { (1UL, Internals(ulong.MaxValue)) }, true));
    }

    [Fact]
    public void WritingRejectsSectionsThatDoNotHoldTheirLinks()
    {
        static void Unencodable(Section section)
        {
            var packet = new LinksPacket();
            packet.Sections.Add(section);
            var error = Assert.Throws<LinoProtocolException>(() => packet.ToBytes());
            Assert.Equal(LinoProtocolErrorKind.Unencodable, error.Kind);
        }
        Unencodable(DoubletSection(0, ArityRange.Exactly(3), 1, (1, 1)));
        Unencodable(DoubletSection(0, ArityRange.Exactly(0), 1));
        Unencodable(DoubletSection(0, ArityRange.Between(3, 2), 1));
        Unencodable(DoubletSection(0, ArityRange.Exactly(1UL << 62), 1));
        Unencodable(DoubletSection(0, ArityRange.Doublets, 3, (1, 1)));
        Unencodable(DoubletSection(0, ArityRange.Doublets, 1, (256, 0)));
        Unencodable(DoubletSection(ulong.MaxValue, ArityRange.Doublets, 1, (1, 1)));
    }

    private static LinoProtocolException DecodeError(byte[] bytes, DecodeLimits? limits = null)
    {
        var protocol = new BinaryLinoProtocol { Limits = limits ?? DecodeLimits.Default };
        return Assert.Throws<LinoProtocolException>(() => protocol.Decode(bytes));
    }

    /// <summary>Each packet breaks exactly one rule, the one its detail names.</summary>
    [Theory]
    [InlineData("", "empty input")]
    [InlineData("20 00", "unsupported binary header byte 0x20")]
    [InlineData("10 01 06 00", "link 6 refers to Internal(6)")]
    [InlineData("10 01 07 00", "link 6 refers to Internal(7)")]
    [InlineData("10 02 00", "unexpected end of packet")]
    [InlineData("10 00 00", "trailing bytes after the packet")]
    [InlineData("10 ff ff ff ff ff ff ff ff ff 7f", "LEB128 value overflows 64 bits")] // a tenth byte above 1
    [InlineData("10 80 80 80 80 80 80 80 80 80 81", "LEB128 value overflows 64 bits")] // an eleventh byte
    [InlineData("10 01 01 01", "the root link is not a list")]
    [InlineData("10 02 00 02 04 06", "broken element chain")] // the chain ends in marker 2, not null
    [InlineData("10 01 04 02", "broken element chain")]
    [InlineData("10 02 02 00 04 06", "marker 2 used as a value")]
    [InlineData("10 03 01 00 06 00 04 07", "marker 1 cannot start a typed value")]
    [InlineData("12 02 14 05 01 20 02 02 06 00 04 07", "a number needs exactly one value")] // (Number) alone
    [InlineData("10 03 05 00 06 00 04 07", "an identified link needs an id")]
    [InlineData("10 04 00 00 05 06 07 00 04 08", "a link id must be a reference")] // the id is ()
    [InlineData("16 00", "the explicit layout keeps the header width bits clear")]
    [InlineData("12 01 04 05 00", "arity must be at least 1")]
    [InlineData("12 01 18 01 01 05 00", "link length outside the section arity 1..2")] // a link of 6
    [InlineData("12 01 f8 ff ff ff ff ff ff ff ff 01 ff ff ff ff ff ff ff ff ff 01 00", "arity range overflows 64 bits")]
    [InlineData("12 01 24 ff ff ff ff ff ff ff ff ff 01 01", "addresses overflow 64 bits")] // the gap
    [InlineData("12 01 20 ff ff ff ff ff ff ff ff ff 01", "addresses overflow 64 bits")] // the count
    [InlineData("12 01 24 06 01 01 01", "from address 6, found link 7 where 6 belongs")]
    [InlineData("12 02 24 05 01 24 01 01 01 01 04 06", "found link 8 where 7 belongs")] // a hole
    public void MalformedPacketsAreRejected(string hex, string detail)
    {
        var error = DecodeError(Unhex(hex));
        Assert.Equal(LinoProtocolErrorKind.Malformed, error.Kind);
        Assert.Contains(detail, error.Detail, StringComparison.Ordinal);
    }

    private static List<(ulong Address, PacketReference[] References)> FromAddress6(params PacketReference[][] links) =>
        links.Select((references, index) => (6UL + (ulong)index, references)).ToList();

    [Fact]
    public void InvalidCodePointsAreRejected()
    {
        // (String (0x110000)) is not a valid code point.
        var links = FromAddress6(
            new[] { PacketReference.External(0x11_0000), PacketReference.Null },
            Internals(3, 6),
            new[] { PacketReference.Internal(7), PacketReference.Null },
            Internals(4, 8));
        Assert.Equal("invalid code point 1114112", DecodeError(LinksPacket.Pack(true, links, false).ToBytes()).Detail);
    }

    [Fact]
    public void HostilePacketsHitLimits()
    {
        void Limited(string hex, DecodeLimits limits) =>
            Assert.True(LinoProtocolErrorKind.LimitExceeded == DecodeError(Unhex(hex), limits).Kind, hex);
        var fewLinks = new DecodeLimits { MaxLinks = 8 };
        Limited("10 09", fewLinks);
        Limited("12 09", fewLinks); // more sections than links allowed
        Limited("12 02 20 05 20 05", fewLinks); // 10 in all
        var fewReferences = new DecodeLimits { MaxReferences = 5 };
        Limited("10 03 01 01 01 01 01 01", fewReferences);
        Limited("12 01 18 00 01 05", fewReferences); // one link of 6

        // A doubling chain `d(k) = (d(k-1) d(k-1))` expands to 2^k nodes.
        var chain = new List<PacketReference[]> { new[] { PacketReference.Null, PacketReference.Null } };
        for (ulong address = 6; address < 60; address++)
        {
            chain.Add(Internals(address, address));
        }
        var last = 5 + (ulong)chain.Count;
        chain.Add(new[] { PacketReference.Internal(last), PacketReference.Null });
        chain.Add(Internals(4, last + 1));
        var bomb = LinksPacket.Pack(false, FromAddress6(chain.ToArray()), false);
        Assert.Equal("limit exceeded: too many LiNo nodes", DecodeError(bomb.ToBytes(), new DecodeLimits { MaxNodes = 1 << 12 }).Message);

        var deep = string.Concat(Enumerable.Repeat("(y ", 40)) + "x" + new string(')', 40);
        var bytes = new BinaryLinoProtocol().Encode(Parse(deep));
        Assert.Equal("limit exceeded: nesting deeper than 10", DecodeError(bytes, new DecodeLimits { MaxDepth = 10 }).Message);

        var threeLinks = new BinaryLinoProtocol().Encode(Parse("a b c"));
        Assert.Equal("limit exceeded: chain too long", DecodeError(threeLinks, new DecodeLimits { MaxNodes = 2 }).Message);
        Assert.Equal(ulong.MaxValue, DecodeLimits.Unlimited.MaxLinks);
    }
}
