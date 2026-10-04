using Foundation.Data.Doublets.Cli.Protocol;
using Platform.Data;

using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

/// <summary>Binary and text LiNo protocol codecs (issue #105).</summary>
public sealed class LinoProtocolCodecTests
{
    private static readonly string[] Corpus =
    {
        "() ((1 1))",
        "((1: 1 1)) ((1: 1 2))",
        "((1 1)) ()",
        "((1: 1 1)) ()",
        "(($i: $s $t)) (($i: $s $t))",
        "((($index: $source $target)) (($index: $target $source)))",
        "(a b c d)",
        "(name: 'with space' \"it's\")",
        "((a))",
        "(((a)))",
        "(a (b c) ((d)))",
        "1\n2\n3",
        "hello",
        "😀 привет 世界",
        "0",
        "007",
        "9223372036854775807",
        "9223372036854775808",
        "18446744073709551615",
        "18446744073709551616",
        "'multi\nline'",
        ".dot",
        "'a''b\"c`d'",
        "(1: (2: 3 4) 5)",
        "() ()",
        "(* *)",
        "(type: type type)",
    };

    /// <summary>Golden vectors shared with the Rust test suite: both ports must produce exactly these bytes.</summary>
    public static TheoryData<string, bool, bool, string> GoldenVectors => new()
    {
        { "() ((1 1))", false, false, "10 07 02 01 06 06 07 00 04 08 00 09 0a 00 04 0b" },
        { "() ((1 1))", false, true, "12 02 03 02 01 06 06 01 07 02 00 08 01 09" },
        { "() ((1 1))", true, false, "11 06 ff ff 06 00 04 07 00 08 09 00 04 0a" },
        { "() ((1 1))", true, true, "13 01 03 ff ff 01 06 02 00 07 01 08" },
        { "hi", true, false, "11 05 97 00 98 06 03 07 08 00 04 09" },
        { "hi", true, true, "13 00 02 03 03 98 97 01 06" },
        { "", false, false, "10 00" },
        { "", true, true, "13 00 00" },
    };

    internal static IEnumerable<BinaryLinoOptions> AllOptions()
    {
        foreach (var externalReferences in new[] { false, true })
        {
            foreach (var sequences in new[] { false, true })
            {
                foreach (var progressiveWidths in new[] { false, true })
                {
                    yield return new BinaryLinoOptions(externalReferences, sequences, progressiveWidths);
                }
            }
        }
    }

    private static string Hex(byte[] bytes) => string.Join(" ", bytes.Select(value => value.ToString("x2")));

    private static byte[] Unhex(string text) =>
        text.Split(' ', StringSplitOptions.RemoveEmptyEntries).Select(value => Convert.ToByte(value, 16)).ToArray();

    private static IReadOnlyList<LinoLink> Parse(string text) => LinoFormat.ParseDocument(text);

    [Theory]
    [MemberData(nameof(GoldenVectors))]
    public void GoldenVectorsAreStable(string text, bool externalReferences, bool sequences, string expected)
    {
        var binary = new BinaryLinoProtocol(new BinaryLinoOptions(externalReferences, sequences));
        Assert.Equal(expected, Hex(binary.Encode(Parse(text))));
        Assert.Equal(Parse(text), new BinaryLinoProtocol().Decode(Unhex(expected)));
    }

    [Fact]
    public void EveryOptionSetRoundTripsTheCorpus()
    {
        foreach (var text in Corpus)
        {
            var document = Parse(text);
            foreach (var options in AllOptions())
            {
                var binary = new BinaryLinoProtocol(options);
                var bytes = binary.Encode(document);
                Assert.InRange(bytes[0], 0x10, 0x1F);
                Assert.True(document.SequenceEqual(binary.Decode(bytes)), $"{text} with {options}");
            }
        }
    }

    [Fact]
    public void CanonicalTextRoundTripsTheCorpus()
    {
        foreach (var text in Corpus)
        {
            var document = Parse(text);
            var canonical = LinoFormat.FormatDocument(document);
            Assert.True(document.SequenceEqual(Parse(canonical)), $"{text} -> {canonical}");
        }
        Assert.Equal("() ((1 1))", LinoFormat.FormatDocument(Parse("(()((1 1)))")));
        Assert.Equal("((1: 1 1)) ((1: 1 2))", LinoFormat.FormatDocument(Parse("((1: 1 1)) ((1: 1 2))")));
    }

    [Fact]
    public void ParsedDocumentsMatchTheRustModel()
    {
        // The C# parser wraps single references in an extra group; the
        // canonical model drops it so both ports decode to equal documents.
        // Expected structures are the output of rust/examples/lino_structure_probe.rs.
        var a = LinoFormat.Reference("a");
        LinoLink Group(LinoLink value) => LinoFormat.Link(null, new List<LinoLink> { value });
        Assert.Equal(new[] { a }, Parse("a"));
        Assert.Equal(new[] { a }, Parse("(a)"));
        Assert.Equal(new[] { Group(a) }, Parse("((a))"));
        Assert.Equal(new[] { Group(Group(a)) }, Parse("(((a)))"));
        Assert.Equal(new[] { LinoFormat.Link("a", new List<LinoLink> { LinoFormat.Reference("b") }) }, Parse("(a: (b))"));
        Assert.Equal(new[] { LinoFormat.Reference("1"), LinoFormat.Reference("2"), LinoFormat.Reference("3") }, Parse("1\n2\n3"));
    }

    [Fact]
    public void ReferencesAreQuotedOnlyWhenNeeded()
    {
        Assert.Equal("plain", LinoFormat.FormatReference("plain"));
        Assert.Equal("$x", LinoFormat.FormatReference("$x"));
        Assert.Equal("''", LinoFormat.FormatReference(""));
        Assert.Equal("'a b'", LinoFormat.FormatReference("a b"));
        Assert.Equal("\"it's\"", LinoFormat.FormatReference("it's"));
        Assert.Equal("`'\"`", LinoFormat.FormatReference("'\""));
        Assert.Equal("\"\"\"'\"`\"\"\"", LinoFormat.FormatReference("'\"`"));
        foreach (var reference in new[]
        {
            "", "a b", "a:b", "(x)", "it's", "'\"", "'\"`", "tab\there", "a\nb", "'x", "x'", "'", "''", "'''",
            "\"'`x`'\"", "a''''b\"\"`", "`'\"\"\"'''``", "(a) ''",
        })
        {
            var formatted = LinoFormat.FormatReference(reference);
            var document = Parse(formatted);
            Assert.Equal(formatted, LinoFormat.FormatDocument(document));
            Assert.Equal(new[] { LinoFormat.Reference(reference) }, document);
        }
    }

    [Fact]
    public void EveryShortReferenceOverDelimitersRoundTrips()
    {
        var alphabet = new[] { '\'', '"', '`', 'a', ' ', '(', ')', ':' };
        var stack = new Stack<string>();
        stack.Push(string.Empty);
        while (stack.TryPop(out var reference))
        {
            if (reference.Length > 0)
            {
                var formatted = LinoFormat.FormatReference(reference);
                var document = Parse(formatted);
                Assert.True(
                    document.Count == 1 && document[0].Equals(LinoFormat.Reference(reference)),
                    $"{reference} as {formatted}");
            }
            if (reference.Length < 5)
            {
                foreach (var character in alphabet)
                {
                    stack.Push(reference + character);
                }
            }
        }
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

    [Fact]
    public void UniformWidthGrowsPast256Addresses()
    {
        var small = LinoMapping.EncodeDocument(Parse(ManyLinks(3)));
        Assert.True(small.LastAddress <= 255);
        Assert.Equal(1, small.MinWidth);

        var document = Parse(ManyLinks(400));
        var uniform = LinoMapping.EncodeDocument(document);
        Assert.True(uniform.LastAddress > 255);
        Assert.Equal(2, uniform.MinWidth);
        var uniformBytes = uniform.ToBytes();
        const int header = 1 + 2; // header byte + two-byte LEB128 count
        Assert.Equal(header + uniform.Doublets.Count * 4, uniformBytes.Length);

        var progressive = LinoMapping.EncodeDocument(document, new BinaryLinoOptions().WithProgressiveWidths());
        Assert.Equal(1, progressive.MinWidth);
        var progressiveBytes = progressive.ToBytes();
        Assert.True(progressiveBytes.Length < uniformBytes.Length);
        foreach (var bytes in new[] { uniformBytes, progressiveBytes })
        {
            Assert.Equal(document, LinoMapping.DecodeDocument(LinksPacket.FromBytes(bytes)));
        }
    }

    [Fact]
    public void LargeExternalValuesRaiseTheMinimumWidth()
    {
        var options = new BinaryLinoOptions(ExternalReferences: true, ProgressiveWidths: true);
        Assert.Equal(2, LinoMapping.EncodeDocument(Parse("(1000 1)"), options).MinWidth);
        Assert.Equal(1, LinoMapping.EncodeDocument(Parse("(100 1)"), options).MinWidth);
        // Beyond 63 bits the number falls back to in-band unary links.
        Assert.True(LinoMapping.EncodeDocument(Parse("18446744073709551615"), options).Doublets.Count > 60);
    }

    [Fact]
    public void HandBuiltPacketsDecode()
    {
        // One doublet (One One) is 2^1 in unary, so `(Number 7)` is the number 2.
        var packet = new LinksPacket();
        packet.Doublets.Add((PacketReference.Internal(1), PacketReference.Internal(1)));
        packet.Doublets.Add((PacketReference.Internal(2), PacketReference.Internal(6)));
        packet.Doublets.Add((PacketReference.Internal(7), PacketReference.Null));
        packet.Doublets.Add((PacketReference.Internal(4), PacketReference.Internal(8)));
        var bytes = packet.ToBytes();
        Assert.Equal("10 04 01 01 02 06 07 00 04 08", Hex(bytes));
        Assert.Equal("2", LinoFormat.FormatDocument(new BinaryLinoProtocol().Decode(bytes)));
    }

    private static LinoProtocolErrorKind DecodeError(byte[] bytes, DecodeLimits? limits = null)
    {
        var protocol = new BinaryLinoProtocol { Limits = limits ?? DecodeLimits.Default };
        return Assert.Throws<LinoProtocolException>(() => protocol.Decode(bytes)).Kind;
    }

    [Theory]
    [InlineData("")]
    [InlineData("20 00")] // unknown version nibble
    [InlineData("10 01 06 00")] // refers to itself
    [InlineData("10 01 07 00")] // refers forward
    [InlineData("10 02 00")] // truncated
    [InlineData("10 00 00")] // trailing byte
    [InlineData("10 ff ff ff ff ff ff ff ff ff 7f")] // LEB128 overflow
    [InlineData("10 01 01 01")] // root is not a list
    [InlineData("10 02 00 02 04 06")] // marker 2 used as a value
    [InlineData("10 01 04 02")] // a bare marker in a list chain
    public void MalformedPacketsAreRejected(string hex)
    {
        Assert.Equal(LinoProtocolErrorKind.Malformed, DecodeError(Unhex(hex)));
    }

    [Fact]
    public void InvalidCodePointsAreRejected()
    {
        // (String (0x110000)) is not a valid code point.
        var packet = new LinksPacket { ExternalReferences = true, MinWidth = 4 };
        packet.Doublets.Add((PacketReference.External(0x11_0000), PacketReference.Null));
        packet.Doublets.Add((PacketReference.Internal(3), PacketReference.Internal(6)));
        packet.Doublets.Add((PacketReference.Internal(7), PacketReference.Null));
        packet.Doublets.Add((PacketReference.Internal(4), PacketReference.Internal(8)));
        Assert.Equal(LinoProtocolErrorKind.Malformed, DecodeError(packet.ToBytes()));
    }

    [Fact]
    public void HostilePacketsHitLimits()
    {
        Assert.Equal(LinoProtocolErrorKind.LimitExceeded, DecodeError(Unhex("10 09"), new DecodeLimits { MaxLinks = 8 }));

        // A doubling chain `d(k) = (d(k-1) d(k-1))` expands to 2^k nodes.
        var bomb = new LinksPacket();
        bomb.Doublets.Add((PacketReference.Null, PacketReference.Null));
        for (ulong address = 6; address < 60; address++)
        {
            bomb.Doublets.Add((PacketReference.Internal(address), PacketReference.Internal(address)));
        }
        var last = 5 + (ulong)bomb.Doublets.Count;
        bomb.Doublets.Add((PacketReference.Internal(last), PacketReference.Null));
        bomb.Doublets.Add((PacketReference.Internal(4), PacketReference.Internal(last + 1)));
        Assert.Equal(LinoProtocolErrorKind.LimitExceeded, DecodeError(bomb.ToBytes(), new DecodeLimits { MaxNodes = 1 << 12 }));

        var deep = string.Concat(Enumerable.Repeat("(y ", 40)) + "x" + new string(')', 40);
        var bytes = new BinaryLinoProtocol().Encode(Parse(deep));
        Assert.Equal(LinoProtocolErrorKind.LimitExceeded, DecodeError(bytes, new DecodeLimits { MaxDepth = 10 }));
    }

    [Fact]
    public void TextMessagesAreDotStuffed()
    {
        var document = Parse(".dot\n'first\n.second'\n(a b)");
        using var wire = new MemoryStream();
        new TextLinoProtocol().WriteDocument(wire, document);
        Assert.Equal("..dot\n'first\n..second'\na b\n.\n", System.Text.Encoding.UTF8.GetString(wire.ToArray()));
        var reader = LinoStreamReader.FromBytes(wire.ToArray());
        Assert.Equal(document, new TextLinoProtocol().ReadDocument(reader));
        Assert.Null(new TextLinoProtocol().ReadDocument(reader));
    }

    [Fact]
    public void TextMessagesAcceptCrlfAndRejectTruncation()
    {
        var crlf = LinoStreamReader.FromBytes("() ((1 1))\r\n.\r\n"u8.ToArray());
        Assert.Equal("() ((1 1))", LinoFormat.FormatDocument(new TextLinoProtocol().ReadDocument(crlf)!));

        Assert.Empty(new TextLinoProtocol().ReadDocument(LinoStreamReader.FromBytes(".\n"u8.ToArray()))!);

        var truncated = LinoStreamReader.FromBytes("() ((1 1))\n"u8.ToArray());
        var error = Assert.Throws<LinoProtocolException>(() => new TextLinoProtocol().ReadDocument(truncated));
        Assert.Equal(LinoProtocolErrorKind.Malformed, error.Kind);

        var tiny = new TextLinoProtocol { Limits = new DecodeLimits { MaxTextBytes = 8 } };
        var longMessage = LinoStreamReader.FromBytes(System.Text.Encoding.UTF8.GetBytes(new string('a', 100) + "\n.\n"));
        Assert.Equal(LinoProtocolErrorKind.LimitExceeded, Assert.Throws<LinoProtocolException>(() => tiny.ReadDocument(longMessage)).Kind);

        var invalidUtf8 = LinoStreamReader.FromBytes(new byte[] { 0xC3, 0x28, (byte)'\n', (byte)'.', (byte)'\n' });
        Assert.Equal(LinoProtocolErrorKind.Malformed, Assert.Throws<LinoProtocolException>(() => new TextLinoProtocol().ReadDocument(invalidUtf8)).Kind);
    }

    [Fact]
    public void ProtocolsAreDetectedPerMessage()
    {
        var document = Parse("() ((1 1))");
        var options = new BinaryLinoOptions().WithExternalReferences().WithSequences();
        using var wire = new MemoryStream();
        new TextLinoProtocol().WriteDocument(wire, document);
        new BinaryLinoProtocol(options).WriteDocument(wire, document);
        new TextLinoProtocol().WriteDocument(wire, document);

        var reader = LinoStreamReader.FromBytes(wire.ToArray());
        var formats = new List<MessageFormat>();
        while (LinoProtocols.ReadAnyDocument(reader) is { } message)
        {
            Assert.Equal(document, message.Document);
            formats.Add(message.Format);
        }
        Assert.Equal(new[] { MessageFormat.Text, MessageFormat.Binary(options), MessageFormat.Text }, formats);
    }

    [Fact]
    public void DeeplyNestedTextParsesInLinearTime()
    {
        // links-notation before 0.21.3 took exponential time in the nesting
        // depth (link-foundation/links-notation#314), so one short message
        // could stall a server thread. Depth 40 took minutes there.
        static string Nested(int depth) => new string('(', depth) + "a" + new string(')', depth);
        var started = System.Diagnostics.Stopwatch.StartNew();
        var link = LinoFormat.ParseDocument(Nested(40)).Single();
        var depth = 1;
        while (link.Values is { } values)
        {
            link = values[0];
            depth++;
        }
        Assert.Equal(40, depth);
        var error = Assert.Throws<LinoProtocolException>(() => LinoFormat.ParseDocument(Nested(100_000)));
        Assert.Equal(LinoProtocolErrorKind.InvalidLino, error.Kind);
        Assert.True(started.Elapsed < TimeSpan.FromSeconds(10), started.Elapsed.ToString());
    }
}
