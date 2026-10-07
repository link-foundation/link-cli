using Foundation.Data.Doublets.Cli.Protocol;
using Link.Foundation.Links.Notation.Binary;

using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

/// <summary>Binary and text LiNo protocol codecs (issue #105).</summary>
public sealed class LinoProtocolCodecTests
{
    internal static readonly string[] Corpus =
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

    internal static IEnumerable<BinaryLinoOptions> AllOptions()
    {
        foreach (var externalReferences in new[] { false, true })
        {
            foreach (var arity in new[] { ArityRange.Doublets, ArityRange.Between(2, 3), ArityRange.AtLeast(1) })
            {
                foreach (var packedWidths in new[] { false, true })
                {
                    yield return new BinaryLinoOptions
                    {
                        ExternalReferences = externalReferences,
                        Arity = arity,
                        PackedWidths = packedWidths,
                    };
                }
            }
        }
    }

    private static IReadOnlyList<LinoLink> Parse(string text) => LinoFormat.ParseDocument(text);

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
        // Only a binary message carries an id without values; links-notation reads its text back as the reference.
        Assert.Equal("(a:)", LinoFormat.FormatDocument(new[] { LinoFormat.Link("a", new List<LinoLink>()) }));
        Assert.True(Parse("(a:)").SequenceEqual(new[] { LinoFormat.Reference("a") }));
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
    public void TextMessagesAreDotStuffed()
    {
        var document = Parse(".dot\n'first\n.second'\n(a b)");
        using var wire = new MemoryStream();
        new TextLinoProtocol().WriteDocument(wire, document);
        Assert.Equal("..dot\n'first\n..second'\na b\n.\n", System.Text.Encoding.UTF8.GetString(wire.ToArray()));
        var reader = PacketReader.FromBytes(wire.ToArray());
        Assert.Equal(document, new TextLinoProtocol().ReadDocument(reader));
        Assert.Null(new TextLinoProtocol().ReadDocument(reader));
    }

    [Fact]
    public void TextMessagesAcceptCrlfAndRejectTruncation()
    {
        var crlf = PacketReader.FromBytes("() ((1 1))\r\n.\r\n"u8.ToArray());
        Assert.Equal("() ((1 1))", LinoFormat.FormatDocument(new TextLinoProtocol().ReadDocument(crlf)!));

        Assert.Empty(new TextLinoProtocol().ReadDocument(PacketReader.FromBytes(".\n"u8.ToArray()))!);

        foreach (var truncatedText in new[] { "() ((1 1))\n", "() ((1 1))" })
        {
            var truncated = PacketReader.FromBytes(System.Text.Encoding.UTF8.GetBytes(truncatedText));
            var error = Assert.Throws<LinoProtocolException>(() => new TextLinoProtocol().ReadDocument(truncated));
            Assert.Equal("stream ended before the '.' terminator line", error.Detail);
        }

        var tiny = new TextLinoProtocol { MaxTextBytes = 8 };
        var longMessage = PacketReader.FromBytes(System.Text.Encoding.UTF8.GetBytes(new string('a', 100) + "\n.\n"));
        Assert.Equal("limit exceeded: text message longer than 8 bytes", Assert.Throws<LinoProtocolException>(() => tiny.ReadDocument(longMessage)).Message);

        var unlimited = new TextLinoProtocol { MaxTextBytes = ProtocolLimits.Unlimited.MaxTextBytes };
        Assert.Equal("a b", LinoFormat.FormatDocument(unlimited.ReadDocument(PacketReader.FromBytes("a b\n.\n"u8.ToArray()))!));

        var invalidUtf8 = PacketReader.FromBytes(new byte[] { 0xC3, 0x28, (byte)'\n', (byte)'.', (byte)'\n' });
        Assert.Equal("malformed message: text message is not valid UTF-8", Assert.Throws<LinoProtocolException>(() => new TextLinoProtocol().ReadDocument(invalidUtf8)).Message);
    }

    [Fact]
    public void AClosedStreamIsAnIoError()
    {
        var stream = new MemoryStream("() ()\n.\n"u8.ToArray());
        var reader = new PacketReader(stream);
        stream.Dispose();

        var error = Assert.Throws<LinoProtocolException>(() => new TextLinoProtocol().ReadDocument(reader));
        Assert.Equal(LinoProtocolErrorKind.Io, error.Kind);
        // The codec error the reader raised is kept, holding the stream's own error.
        Assert.IsType<ObjectDisposedException>(Assert.IsType<BinaryNotationException>(error.InnerException).InnerException);
    }

    [Fact]
    public void ProtocolErrorsNameTheirKind()
    {
        Assert.Equal("malformed message: unknown protocol error", new LinoProtocolException().Message);
        Assert.Equal(LinoProtocolErrorKind.Malformed, new LinoProtocolException("bad").Kind);
        var inner = new IOException("reset");
        var io = new LinoProtocolException("reset", inner);
        Assert.Equal((LinoProtocolErrorKind.Io, "I/O error: reset", inner), (io.Kind, io.Message, io.InnerException));
        Assert.Equal("invalid LiNo: x", new LinoProtocolException(LinoProtocolErrorKind.InvalidLino, "x").Message);
        Assert.Equal("server error: x", new LinoProtocolException(LinoProtocolErrorKind.Remote, "x").Message);
        Assert.Equal("42: x", new LinoProtocolException((LinoProtocolErrorKind)42, "x").Message);
    }

    [Theory]
    [InlineData(BinaryErrorKind.Io, LinoProtocolErrorKind.Io, "I/O error: x")]
    [InlineData(BinaryErrorKind.Malformed, LinoProtocolErrorKind.Malformed, "malformed message: x")]
    [InlineData(BinaryErrorKind.InvalidLino, LinoProtocolErrorKind.InvalidLino, "invalid LiNo: x")]
    [InlineData(BinaryErrorKind.LimitExceeded, LinoProtocolErrorKind.LimitExceeded, "limit exceeded: x")]
    [InlineData(BinaryErrorKind.Unencodable, LinoProtocolErrorKind.Unencodable, "cannot encode: x")]
    [InlineData((BinaryErrorKind)42, LinoProtocolErrorKind.Malformed, "malformed message: x")]
    public void CodecErrorsKeepTheirKindAndDetail(BinaryErrorKind kind, LinoProtocolErrorKind expected, string message)
    {
        var codecError = new BinaryNotationException(kind, "x");
        var error = LinoProtocolException.From(codecError);
        Assert.Equal((expected, "x", message), (error.Kind, error.Detail, error.Message));
        Assert.Same(codecError, error.InnerException);
    }

    [Fact]
    public void ProtocolsRaiseOnlyLinoProtocolErrors()
    {
        var invalid = PacketReader.FromBytes("(a\n.\n"u8.ToArray());
        var error = Assert.Throws<LinoProtocolException>(() => new TextLinoProtocol().ReadDocument(invalid));
        Assert.Equal(LinoProtocolErrorKind.InvalidLino, error.Kind);
        Assert.IsType<BinaryNotationException>(error.InnerException);

        var tooDeep = new BinaryLinoProtocol { Limits = new DecodeLimits { MaxDepth = 1 } };
        var unencodable = Assert.Throws<LinoProtocolException>(() => tooDeep.Encode(Parse("((a))")));
        Assert.Equal("cannot encode: nesting deeper than 1", unencodable.Message);

        var limits = new ProtocolLimits { Binary = new DecodeLimits { MaxLinks = 1 }, MaxTextBytes = 4 };
        var binary = new BinaryLinoProtocol().Encode(Parse("a"));
        var limited = Assert.Throws<LinoProtocolException>(() => LinoProtocols.ReadAnyDocument(PacketReader.FromBytes(binary), limits));
        Assert.Equal("limit exceeded: packet declares more than 1 links", limited.Message);
        var longText = PacketReader.FromBytes("abcdef\n.\n"u8.ToArray());
        Assert.Equal(LinoProtocolErrorKind.LimitExceeded, Assert.Throws<LinoProtocolException>(() => LinoProtocols.ReadAnyDocument(longText, limits)).Kind);
        Assert.Equal(new TextLinoProtocol { MaxTextBytes = 4 }, MessageFormat.Text.Protocol(limits));
        Assert.Equal(new BinaryLinoProtocol { Limits = limits.Binary }, MessageFormat.Binary(default).Protocol(limits));
        Assert.Equal((DecodeLimits.Unlimited, long.MaxValue), (ProtocolLimits.Unlimited.Binary, ProtocolLimits.Unlimited.MaxTextBytes));
        Assert.Equal(new ProtocolLimits(), ProtocolLimits.Default);

        var stream = new MemoryStream(binary);
        var closed = new PacketReader(stream);
        stream.Dispose();
        Assert.Equal(LinoProtocolErrorKind.Io, Assert.Throws<LinoProtocolException>(() => LinoProtocols.ReadAnyDocument(closed)).Kind);
        Assert.Equal(LinoProtocolErrorKind.Io, Assert.Throws<LinoProtocolException>(() => new BinaryLinoProtocol().ReadDocument(closed)).Kind);
    }

    [Fact]
    public void ProtocolsAreDetectedPerMessage()
    {
        var document = Parse("() ((1 1))");
        var options = new BinaryLinoOptions().WithExternalReferences();
        using var wire = new MemoryStream();
        new TextLinoProtocol().WriteDocument(wire, document);
        new BinaryLinoProtocol(options).WriteDocument(wire, document);
        new TextLinoProtocol().WriteDocument(wire, document);

        var reader = PacketReader.FromBytes(wire.ToArray());
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
        var error = Assert.Throws<BinaryNotationException>(() => LinoFormat.ParseDocument(Nested(100_000)));
        Assert.Equal(BinaryErrorKind.InvalidLino, error.Kind);
        Assert.True(started.Elapsed < TimeSpan.FromSeconds(10), started.Elapsed.ToString());
    }
}
