namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>One IPC mechanism as a reliable, ordered, duplex byte channel (issue #140).</summary>
/// <remarks>Every member may be called from several threads at once: one thread reads while another writes.</remarks>
public interface ITransport
{
    /// <summary>
    /// Reads at least one byte, waiting up to <paramref name="timeout"/> (null waits forever); 0 at the end of the
    /// incoming direction. A timeout raises an <see cref="IpcErrorKind.TimedOut"/> error.
    /// </summary>
    int Read(Span<byte> buffer, TimeSpan? timeout);

    /// <summary>Writes at least one byte of a non-empty buffer and returns how many.</summary>
    int Write(ReadOnlySpan<byte> buffer);

    /// <summary>Sends everything written so far.</summary>
    void Flush() { }

    /// <summary>Ends the outgoing direction; the peer then reads the end of its stream.</summary>
    void CloseWrite();

    /// <summary>Hangs up both directions; idempotent.</summary>
    void Shutdown();

    /// <summary>The endpoint kind, such as <c>shm</c>.</summary>
    string Kind { get; }
}

/// <summary>
/// A duplex byte <see cref="Stream"/> over any <see cref="ITransport"/>: what the LiNo protocols read and write
/// whatever the mechanism under them.
/// </summary>
/// <remarks>
/// <see cref="Clone"/> gives another handle on the same channel with a read timeout of its own, so one thread can
/// read while another writes. The transport shuts down when the last handle is disposed.
/// </remarks>
public sealed class IpcStream : Stream
{
    private sealed class Shared(ITransport transport)
    {
        public readonly ITransport Transport = transport;
        public int Handles = 1;
    }

    private readonly Shared _shared;
    private int _disposed;
    private int _readTimeout = System.Threading.Timeout.Infinite;

    /// <summary>Wraps <paramref name="transport"/>, which the stream then owns.</summary>
    public IpcStream(ITransport transport)
    {
        ArgumentNullException.ThrowIfNull(transport);
        _shared = new Shared(transport);
    }

    private IpcStream(Shared shared)
    {
        _shared = shared;
    }

    /// <summary>The transport under the stream.</summary>
    public ITransport Transport => _shared.Transport;

    /// <summary>The endpoint kind of the transport.</summary>
    public string Kind => _shared.Transport.Kind;

    /// <summary>Another handle on the same channel; the read timeout starts as this one's.</summary>
    public IpcStream Clone()
    {
        ObjectDisposedException.ThrowIf(Volatile.Read(ref _disposed) != 0, this);
        Interlocked.Increment(ref _shared.Handles);
        return new IpcStream(_shared) { _readTimeout = _readTimeout };
    }

    /// <summary>The read timeout of this handle as a duration; null waits forever.</summary>
    public TimeSpan? ReadTimeoutSpan
    {
        get => _readTimeout == System.Threading.Timeout.Infinite ? null : TimeSpan.FromMilliseconds(_readTimeout);
        set => _readTimeout = value is { } timeout
            ? (int)Math.Clamp(Math.Ceiling(timeout.TotalMilliseconds), 0, int.MaxValue)
            : System.Threading.Timeout.Infinite;
    }

    /// <inheritdoc/>
    public override bool CanTimeout => true;

    /// <inheritdoc/>
    public override int ReadTimeout
    {
        get => _readTimeout;
        set => _readTimeout = value < 0 ? System.Threading.Timeout.Infinite : value;
    }

    /// <inheritdoc/>
    public override bool CanRead => true;

    /// <inheritdoc/>
    public override bool CanWrite => true;

    /// <inheritdoc/>
    public override bool CanSeek => false;

    /// <inheritdoc/>
    public override long Length => throw new NotSupportedException();

    /// <inheritdoc/>
    public override long Position
    {
        get => throw new NotSupportedException();
        set => throw new NotSupportedException();
    }

    /// <summary>Ends the outgoing direction.</summary>
    public void CloseWrite() => _shared.Transport.CloseWrite();

    /// <summary>Hangs up both directions, for every handle.</summary>
    public void Shutdown() => _shared.Transport.Shutdown();

    /// <inheritdoc/>
    public override int Read(byte[] buffer, int offset, int count) => Read(buffer.AsSpan(offset, count));

    /// <inheritdoc/>
    public override int Read(Span<byte> buffer)
    {
        ObjectDisposedException.ThrowIf(Volatile.Read(ref _disposed) != 0, this);
        if (buffer.IsEmpty)
        {
            return 0;
        }
        while (true)
        {
            try
            {
                return _shared.Transport.Read(buffer, ReadTimeoutSpan);
            }
            catch (ThreadInterruptedException)
            {
                // As an interrupted read of the Rust stream: try again.
            }
        }
    }

    /// <inheritdoc/>
    public override void Write(byte[] buffer, int offset, int count) => Write(buffer.AsSpan(offset, count));

    /// <inheritdoc/>
    public override void Write(ReadOnlySpan<byte> buffer)
    {
        ObjectDisposedException.ThrowIf(Volatile.Read(ref _disposed) != 0, this);
        while (!buffer.IsEmpty)
        {
            var written = _shared.Transport.Write(buffer);
            if (written <= 0)
            {
                throw new IpcException(IpcErrorKind.BrokenPipe, "failed to write the whole buffer");
            }
            buffer = buffer[written..];
        }
    }

    /// <inheritdoc/>
    public override void Flush()
    {
        ObjectDisposedException.ThrowIf(Volatile.Read(ref _disposed) != 0, this);
        _shared.Transport.Flush();
    }

    /// <inheritdoc/>
    public override long Seek(long offset, SeekOrigin origin) => throw new NotSupportedException();

    /// <inheritdoc/>
    public override void SetLength(long value) => throw new NotSupportedException();

    /// <inheritdoc/>
    public override string ToString() => $"IpcStream({Kind})";

    /// <inheritdoc/>
    protected override void Dispose(bool disposing)
    {
        if (Interlocked.Exchange(ref _disposed, 1) == 0 && Interlocked.Decrement(ref _shared.Handles) == 0)
        {
            _shared.Transport.Shutdown();
            (_shared.Transport as IDisposable)?.Dispose();
        }
        base.Dispose(disposing);
    }
}
