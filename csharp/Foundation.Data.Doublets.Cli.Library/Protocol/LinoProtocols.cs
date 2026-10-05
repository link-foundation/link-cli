using System.Text;

using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>
/// Reads and writes whole LiNo documents on a byte stream. Both
/// <see cref="TextLinoProtocol"/> and <see cref="BinaryLinoProtocol"/>
/// implement it, so code written against it switches protocol by swapping one value.
/// </summary>
public interface ILinoProtocol
{
    /// <summary>Writes one message carrying <paramref name="document"/>.</summary>
    void WriteDocument(Stream output, IReadOnlyList<LinoLink> document);

    /// <summary>Reads one message; <c>null</c> when the stream ended cleanly.</summary>
    IReadOnlyList<LinoLink>? ReadDocument(LinoStreamReader input);
}

/// <summary>
/// UTF-8 LiNo text, one message per block of lines ended by a line holding
/// only <c>.</c>. Lines starting with <c>.</c> get one extra <c>.</c> (SMTP dot-stuffing).
/// </summary>
/// <remarks>
/// Lines may end with <c>\n</c> or <c>\r\n</c>; a carriage return right before
/// a line feed is dropped, so a quoted reference holding <c>\r\n</c> arrives as
/// <c>\n</c> (use <see cref="BinaryLinoProtocol"/> to carry such references exactly).
/// </remarks>
public sealed record TextLinoProtocol : ILinoProtocol
{
    private static readonly UTF8Encoding StrictUtf8 = new(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);

    /// <summary>Limits applied when reading.</summary>
    public DecodeLimits Limits { get; init; } = DecodeLimits.Default;

    /// <summary>Frames already formatted LiNo text as one message.</summary>
    public static void WriteText(Stream output, string text)
    {
        ArgumentNullException.ThrowIfNull(output);
        ArgumentNullException.ThrowIfNull(text);
        var framed = new StringBuilder(text.Length + 4);
        if (text.Length > 0)
        {
            foreach (var line in text.Split('\n'))
            {
                if (line.StartsWith('.'))
                {
                    framed.Append('.');
                }
                framed.Append(line).Append('\n');
            }
        }
        framed.Append(".\n");
        var bytes = StrictUtf8.GetBytes(framed.ToString());
        output.Write(bytes);
        output.Flush();
    }

    /// <summary>Reads one framed message as raw text, without parsing it.</summary>
    public string? ReadText(LinoStreamReader input)
    {
        ArgumentNullException.ThrowIfNull(input);
        var text = new List<byte>();
        var line = new List<byte>();
        var first = true;
        while (true)
        {
            line.Clear();
            var budget = Math.Max(0, Limits.MaxTextBytes - text.Count) + 2;
            var read = input.ReadLine(line, budget);
            if (read == 0)
            {
                if (first)
                {
                    return null;
                }
                throw LinoProtocolException.Malformed("stream ended before the '.' terminator line");
            }
            if (line[^1] != (byte)'\n')
            {
                if (read >= budget)
                {
                    throw LinoProtocolException.Limit($"text message longer than {Limits.MaxTextBytes} bytes");
                }
                throw LinoProtocolException.Malformed("stream ended before the '.' terminator line");
            }
            line.RemoveAt(line.Count - 1);
            // CRLF line endings (telnet, netcat on Windows) are accepted.
            if (line.Count > 0 && line[^1] == (byte)'\r')
            {
                line.RemoveAt(line.Count - 1);
            }
            if (line.Count == 1 && line[0] == (byte)'.')
            {
                break;
            }
            if (!first)
            {
                text.Add((byte)'\n');
            }
            first = false;
            var skip = line.Count > 0 && line[0] == (byte)'.' ? 1 : 0;
            text.AddRange(line.Skip(skip));
        }
        try
        {
            return StrictUtf8.GetString(text.ToArray());
        }
        catch (DecoderFallbackException)
        {
            throw LinoProtocolException.Malformed("text message is not valid UTF-8");
        }
    }

    /// <inheritdoc/>
    public void WriteDocument(Stream output, IReadOnlyList<LinoLink> document) =>
        WriteText(output, LinoFormat.FormatDocument(document));

    /// <inheritdoc/>
    public IReadOnlyList<LinoLink>? ReadDocument(LinoStreamReader input) =>
        ReadText(input) is { } text ? LinoFormat.ParseDocument(text) : null;
}

/// <summary>
/// Binary links packets (see <see cref="LinksPacket"/>); every message is
/// self-delimiting, so no extra framing is needed.
/// </summary>
public sealed record BinaryLinoProtocol : ILinoProtocol
{
    /// <summary>A binary protocol with every optional feature off.</summary>
    public BinaryLinoProtocol()
    {
    }

    /// <summary>A binary protocol with the given optional features.</summary>
    public BinaryLinoProtocol(BinaryLinoOptions options) => Options = options;

    /// <summary>Optional features used when writing.</summary>
    public BinaryLinoOptions Options { get; init; }

    /// <summary>Limits applied when reading.</summary>
    public DecodeLimits Limits { get; init; } = DecodeLimits.Default;

    /// <summary>Encodes a document into packet bytes.</summary>
    public byte[] Encode(IReadOnlyList<LinoLink> document) => LinoMapping.EncodeDocument(document, Options).ToBytes();

    /// <summary>Decodes packet bytes into a document.</summary>
    public IReadOnlyList<LinoLink> Decode(byte[] bytes) =>
        LinoMapping.DecodeDocument(LinksPacket.FromBytes(bytes, Limits), Limits);

    /// <inheritdoc/>
    public void WriteDocument(Stream output, IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(output);
        output.Write(Encode(document));
        output.Flush();
    }

    /// <inheritdoc/>
    public IReadOnlyList<LinoLink>? ReadDocument(LinoStreamReader input) =>
        LinksPacket.ReadFrom(input, Limits) is { } packet ? LinoMapping.DecodeDocument(packet, Limits) : null;
}

/// <summary>The wire format a message arrived in, so a reply can use the same one.</summary>
/// <param name="IsBinary">False for <see cref="TextLinoProtocol"/>.</param>
/// <param name="Options">The binary options read from the packet header.</param>
public readonly record struct MessageFormat(bool IsBinary, BinaryLinoOptions Options = default)
{
    /// <summary>The text format.</summary>
    public static MessageFormat Text => new(false);

    /// <summary>The binary format with <paramref name="options"/>.</summary>
    public static MessageFormat Binary(BinaryLinoOptions options) => new(true, options);

    /// <summary>A protocol that writes messages in this format.</summary>
    public ILinoProtocol Protocol(DecodeLimits limits) =>
        IsBinary ? new BinaryLinoProtocol(Options) { Limits = limits } : new TextLinoProtocol { Limits = limits };
}

/// <summary>Detects which protocol a peer used.</summary>
public static class LinoProtocols
{
    /// <summary>True when <paramref name="value"/> starts a binary message rather than a text one.</summary>
    public static bool IsBinaryStart(byte value) => (value & 0xF0) == LinksPacket.BinaryVersion1;

    /// <summary>Reads one message in whichever protocol the peer used; <c>null</c> at the end of the stream.</summary>
    public static (IReadOnlyList<LinoLink> Document, MessageFormat Format)? ReadAnyDocument(
        LinoStreamReader input,
        DecodeLimits? limits = null)
    {
        ArgumentNullException.ThrowIfNull(input);
        limits ??= DecodeLimits.Default;
        var first = input.PeekByte();
        if (first < 0)
        {
            return null;
        }
        if (!IsBinaryStart((byte)first))
        {
            return new TextLinoProtocol { Limits = limits }.ReadDocument(input) is { } text
                ? (text, MessageFormat.Text)
                : null;
        }
        if (LinksPacket.ReadFrom(input, limits) is not { } packet)
        {
            return null;
        }
        return (LinoMapping.DecodeDocument(packet, limits), MessageFormat.Binary(BinaryLinoOptions.OfPacket(packet)));
    }
}

/// <summary>A byte stream decorated with an <see cref="ILinoProtocol"/>.</summary>
public sealed class LinoConnection : IDisposable
{
    private readonly Stream _stream;
    private readonly LinoStreamReader _reader;

    /// <summary>Wraps <paramref name="stream"/>; disposing the connection disposes the stream.</summary>
    public LinoConnection(Stream stream, ILinoProtocol protocol)
    {
        ArgumentNullException.ThrowIfNull(stream);
        ArgumentNullException.ThrowIfNull(protocol);
        _stream = stream;
        _reader = new LinoStreamReader(stream);
        Protocol = protocol;
    }

    /// <summary>The protocol in use.</summary>
    public ILinoProtocol Protocol { get; }

    /// <summary>Sends one document.</summary>
    public void Send(IReadOnlyList<LinoLink> document)
    {
        using var buffer = new MemoryStream();
        Protocol.WriteDocument(buffer, document);
        try
        {
            buffer.WriteTo(_stream);
            _stream.Flush();
        }
        catch (Exception error) when (error is IOException or ObjectDisposedException)
        {
            throw new LinoProtocolException(LinoProtocolErrorKind.Io, error.Message, error);
        }
    }

    /// <summary>Receives one document; <c>null</c> when the peer closed the stream.</summary>
    public IReadOnlyList<LinoLink>? Receive() => Protocol.ReadDocument(_reader);

    /// <summary>Sends <paramref name="document"/> and waits for the reply.</summary>
    public IReadOnlyList<LinoLink> Request(IReadOnlyList<LinoLink> document)
    {
        Send(document);
        return Receive() ?? throw LinoProtocolException.Malformed("connection closed before the reply");
    }

    /// <inheritdoc/>
    public void Dispose() => _stream.Dispose();
}
