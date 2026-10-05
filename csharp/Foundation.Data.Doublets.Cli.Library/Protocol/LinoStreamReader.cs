namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>
/// A buffered byte reader that can peek at the next byte, so a server can
/// tell a text message from a binary one before reading it.
/// </summary>
public sealed class LinoStreamReader
{
    private readonly Stream _stream;
    private readonly byte[] _buffer;
    private int _position;
    private int _length;

    public LinoStreamReader(Stream stream, int bufferSize = 8192)
    {
        ArgumentNullException.ThrowIfNull(stream);
        _stream = stream;
        _buffer = new byte[bufferSize];
    }

    /// <summary>Wraps a byte array, e.g. a whole encoded message.</summary>
    public static LinoStreamReader FromBytes(byte[] bytes) => new(new MemoryStream(bytes, writable: false));

    /// <summary>The next byte without consuming it, or -1 at the end of the stream.</summary>
    public int PeekByte() => Fill() ? _buffer[_position] : -1;

    /// <summary>The next byte, or -1 at the end of the stream.</summary>
    public int ReadByte() => Fill() ? _buffer[_position++] : -1;

    /// <summary>True when every byte of the stream has been consumed.</summary>
    public bool AtEnd => !Fill();

    /// <summary>Fills <paramref name="destination"/>; a short stream is a malformed message.</summary>
    public void ReadExactly(Span<byte> destination)
    {
        while (!destination.IsEmpty)
        {
            if (!Fill())
            {
                throw LinoProtocolException.Malformed("unexpected end of packet");
            }
            var count = Math.Min(destination.Length, _length - _position);
            _buffer.AsSpan(_position, count).CopyTo(destination);
            _position += count;
            destination = destination[count..];
        }
    }

    /// <summary>
    /// Appends bytes up to and including the next <c>\n</c> to <paramref name="line"/>,
    /// reading at most <paramref name="maxBytes"/>. Returns the number of bytes read.
    /// </summary>
    public int ReadLine(List<byte> line, long maxBytes)
    {
        var read = 0;
        while (read < maxBytes && Fill())
        {
            var available = _buffer.AsSpan(_position, (int)Math.Min(_length - _position, maxBytes - read));
            var newline = available.IndexOf((byte)'\n');
            var take = newline < 0 ? available.Length : newline + 1;
            for (var index = 0; index < take; index++)
            {
                line.Add(available[index]);
            }
            _position += take;
            read += take;
            if (newline >= 0)
            {
                break;
            }
        }
        return read;
    }

    private bool Fill()
    {
        if (_position < _length)
        {
            return true;
        }
        try
        {
            _length = _stream.Read(_buffer, 0, _buffer.Length);
        }
        catch (IOException exception)
        {
            throw new LinoProtocolException(LinoProtocolErrorKind.Io, exception.Message, exception);
        }
        catch (ObjectDisposedException exception)
        {
            throw new LinoProtocolException(LinoProtocolErrorKind.Io, exception.Message, exception);
        }
        _position = 0;
        return _length > 0;
    }
}
