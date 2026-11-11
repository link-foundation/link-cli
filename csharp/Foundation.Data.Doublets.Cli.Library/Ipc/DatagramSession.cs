using System.Buffers.Binary;
using System.Diagnostics;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>A mechanism that moves whole frames, possibly losing some.</summary>
internal interface IDatagrams
{
    /// <summary>Sends one frame; false when it was dropped (a full queue).</summary>
    bool Send(ReadOnlySpan<byte> frame);

    /// <summary>Receives one frame, waiting up to <paramref name="timeout"/>; null when none arrived in time.</summary>
    byte[]? Receive(TimeSpan timeout);

    /// <summary>Whether the peer may still answer (process liveness for local kinds).</summary>
    bool PeerAlive => true;

    /// <summary>Releases the mechanism; called once, after the session stopped.</summary>
    void Close() { }
}

/// <summary>The session settings.</summary>
/// <param name="Chunk">The largest payload of one frame.</param>
/// <param name="Rto">The first retransmission timeout.</param>
/// <param name="Heartbeat">Send <c>PING</c> after this much silence (<c>udp</c>).</param>
/// <param name="Dead">The peer is gone after this much silence (<c>udp</c>).</param>
internal readonly record struct DatagramSettings(int Chunk, TimeSpan Rto, TimeSpan? Heartbeat, TimeSpan? Dead)
{
    /// <summary>The settings of a local datagram kind: no heartbeat, the peer's liveness is its process.</summary>
    public static DatagramSettings Local(Endpoint endpoint, int chunk) => new(chunk, endpoint.Millis("rto", 20), null, null);
}

/// <summary>
/// A reliable byte stream over <see cref="IDatagrams"/> (see "Reliable datagram session" in
/// <c>docs/protocol/ipc-transports-v1.md</c>): numbered DATA and EOF frames, each acknowledged before the next.
/// </summary>
internal sealed class DatagramSession : ITransport, IDisposable
{
    public const int HeaderSize = 8;
    public const byte Data = 1;
    public const byte Ack = 2;
    public const byte Eof = 3;
    public const byte Ping = 4;
    public const byte Abort = 5;
    public const byte Hello = 6;
    public const byte Welcome = 7;

    /// <summary>At most this many received bytes wait to be read.</summary>
    private const int Window = 1 << 20;
    private static readonly TimeSpan MaxRto = TimeSpan.FromMilliseconds(1000);
    private static readonly TimeSpan Tick = TimeSpan.FromMilliseconds(100);
    /// <summary>How long hanging up waits for the end-of-stream frame to be acknowledged.</summary>
    private static readonly TimeSpan Linger = TimeSpan.FromMilliseconds(1000);

    private readonly IDatagrams _link;
    private readonly DatagramSettings _settings;
    private readonly object _state = new();
    private readonly object _sending = new();
    private readonly Queue<byte> _inbound = new();
    private uint _received;
    private bool _ended;
    private bool _aborted;
    private uint _sent;
    private uint _acknowledged;
    private bool _writeClosed;
    private bool _closed;
    private bool _stopping;
    private long _lastHeard;
    private long _lastSent;

    /// <summary>Builds one frame.</summary>
    public static byte[] Frame(byte kind, uint seq, ReadOnlySpan<byte> payload)
    {
        var frame = new byte[HeaderSize + payload.Length];
        frame[0] = kind;
        BinaryPrimitives.WriteUInt32LittleEndian(frame.AsSpan(4), seq);
        payload.CopyTo(frame.AsSpan(HeaderSize));
        return frame;
    }

    /// <summary>Splits a frame into its type, sequence number and payload.</summary>
    public static bool TryParse(ReadOnlySpan<byte> frame, out byte kind, out uint seq, out ReadOnlySpan<byte> payload)
    {
        if (frame.Length < HeaderSize)
        {
            kind = 0;
            seq = 0;
            payload = default;
            return false;
        }
        kind = frame[0];
        seq = BinaryPrimitives.ReadUInt32LittleEndian(frame[4..]);
        payload = frame[HeaderSize..];
        return true;
    }

    /// <summary>The chunk both sides use: the smaller proposal.</summary>
    public static int AgreeChunk(ulong own, ulong? peer) => (int)Math.Clamp(peer is { } other ? Math.Min(own, other) : own, 16, 65000);

    public DatagramSession(string kind, IDatagrams link, DatagramSettings settings)
    {
        Kind = kind;
        _link = link;
        _settings = settings;
        _lastHeard = _lastSent = Stopwatch.GetTimestamp();
        new Thread(ReceiveLoop) { IsBackground = true, Name = $"clink-{kind}-session" }.Start();
        IpcTransports.Trace($"{kind} session started, chunk {settings.Chunk}");
    }

    public string Kind { get; }

    private static bool Newer(uint a, uint b) => unchecked(a - b) <= uint.MaxValue / 2;

    private bool SendFrame(ReadOnlySpan<byte> frame)
    {
        var sent = _link.Send(frame);
        lock (_state)
        {
            _lastSent = Stopwatch.GetTimestamp();
        }
        return sent;
    }

    private void ReceiveLoop()
    {
        var tick = Tick;
        if (_settings.Heartbeat is { } heartbeat)
        {
            tick = TimeSpan.FromTicks(Math.Max(TimeSpan.TicksPerMillisecond, Math.Min(Tick.Ticks, heartbeat.Ticks / 2)));
        }
        var lastCheck = Stopwatch.GetTimestamp();
        while (true)
        {
            lock (_state)
            {
                if (_stopping)
                {
                    break;
                }
            }
            try
            {
                if (_link.Receive(tick) is { } frame)
                {
                    Handle(frame);
                }
            }
            catch (Exception error) when (error is IOException or ObjectDisposedException or InvalidOperationException)
            {
                IpcTransports.Trace($"{Kind} receive failed: {error.Message}");
                HangUp();
                Thread.Sleep(tick);
            }
            if (Stopwatch.GetElapsedTime(lastCheck) >= tick)
            {
                lastCheck = Stopwatch.GetTimestamp();
                Housekeeping();
            }
        }
        _link.Close();
        IpcTransports.Trace($"{Kind} session stopped");
    }

    private void Handle(ReadOnlySpan<byte> frame)
    {
        if (!TryParse(frame, out var kind, out var seq, out var payload))
        {
            return;
        }
        uint? reply = null;
        var deliver = false;
        lock (_state)
        {
            _lastHeard = Stopwatch.GetTimestamp();
            switch (kind)
            {
                case Data or Eof:
                    if (seq == unchecked(_received + 1) && !_ended && _inbound.Count <= Window && payload.Length <= _settings.Chunk)
                    {
                        // Only this thread handles frames, so the frame is delivered below, after its acknowledgement.
                        _received = seq;
                        deliver = true;
                        reply = _received;
                    }
                    else if (unchecked(seq - _received) > uint.MaxValue / 2 || seq == _received)
                    {
                        // A retransmission of something already accepted.
                        reply = _received;
                    }
                    break;
                case Ack:
                    if (Newer(seq, _acknowledged))
                    {
                        _acknowledged = seq;
                        Monitor.PulseAll(_state);
                    }
                    break;
                case Abort:
                    if (!_aborted)
                    {
                        IpcTransports.Trace($"{Kind} peer hung up");
                    }
                    _aborted = true;
                    Monitor.PulseAll(_state);
                    break;
            }
        }
        if (reply is { } acknowledged)
        {
            TrySend(Frame(Ack, acknowledged, []));
        }
        if (deliver)
        {
            // The reader sees the frame only once it is acknowledged: a reader that hangs up as soon as it sees the
            // end must not abort the peer before the peer learns its end arrived.
            lock (_state)
            {
                if (kind == Eof)
                {
                    _ended = true;
                }
                else
                {
                    foreach (var b in payload)
                    {
                        _inbound.Enqueue(b);
                    }
                }
                Monitor.PulseAll(_state);
            }
        }
    }

    private void TrySend(ReadOnlySpan<byte> frame)
    {
        try
        {
            SendFrame(frame);
        }
        catch (Exception error) when (error is IOException or ObjectDisposedException or InvalidOperationException)
        {
        }
    }

    private void Housekeeping()
    {
        bool ping, silent;
        lock (_state)
        {
            if (_aborted || _stopping)
            {
                return;
            }
            ping = _settings.Heartbeat is { } heartbeat && Stopwatch.GetElapsedTime(_lastSent) >= heartbeat;
            silent = _settings.Dead is { } dead && Stopwatch.GetElapsedTime(_lastHeard) >= dead;
        }
        if (silent || !_link.PeerAlive)
        {
            IpcTransports.Trace($"{Kind} peer is gone");
            HangUp();
        }
        else if (ping)
        {
            TrySend(Frame(Ping, 0, []));
        }
    }

    private void HangUp()
    {
        lock (_state)
        {
            _aborted = true;
            Monitor.PulseAll(_state);
        }
    }

    /// <summary>Sends a DATA or EOF frame and waits for its acknowledgement.</summary>
    private void SendReliably(byte kind, ReadOnlySpan<byte> payload, Deadline? linger)
    {
        uint seq;
        lock (_state)
        {
            seq = _sent = unchecked(_sent + 1);
        }
        var frame = Frame(kind, seq, payload);
        var rto = _settings.Rto < TimeSpan.FromMilliseconds(1) ? TimeSpan.FromMilliseconds(1) : _settings.Rto;
        while (true)
        {
            SendFrame(frame);
            var deadline = Deadline.After(rto);
            lock (_state)
            {
                while (true)
                {
                    if (Newer(_acknowledged, seq))
                    {
                        return;
                    }
                    if (_aborted)
                    {
                        if (kind == Eof)
                        {
                            // Every DATA frame before the end was acknowledged, so nothing is lost: a peer that hung
                            // up no longer reads, and its acknowledgement of the end may have been the frame lost.
                            IpcTransports.Trace($"{Kind} peer hung up before acknowledging the end");
                            return;
                        }
                        throw IpcException.BrokenPipe();
                    }
                    if (_closed && linger is null)
                    {
                        throw new IpcException(IpcErrorKind.BrokenPipe, "the connection was closed");
                    }
                    if (linger is { Passed: true })
                    {
                        throw IpcException.TimedOut();
                    }
                    if (deadline.Passed)
                    {
                        break;
                    }
                    Monitor.Wait(_state, deadline.Remaining);
                }
            }
            rto = rto * 2 < MaxRto ? rto * 2 : MaxRto;
        }
    }

    public int Read(Span<byte> buffer, TimeSpan? timeout)
    {
        var deadline = Deadline.After(timeout);
        lock (_state)
        {
            while (true)
            {
                if (_inbound.Count > 0)
                {
                    var count = Math.Min(buffer.Length, _inbound.Count);
                    for (var i = 0; i < count; i++)
                    {
                        buffer[i] = _inbound.Dequeue();
                    }
                    return count;
                }
                if (_ended || _closed)
                {
                    return 0;
                }
                if (_aborted)
                {
                    throw IpcException.HungUp();
                }
                var wait = Tick * 10;
                if (deadline is { } end)
                {
                    if (end.Passed)
                    {
                        throw IpcException.TimedOut();
                    }
                    wait = end.Remaining;
                }
                Monitor.Wait(_state, wait);
            }
        }
    }

    public int Write(ReadOnlySpan<byte> data)
    {
        lock (_sending)
        {
            lock (_state)
            {
                if (_writeClosed || _closed)
                {
                    throw new IpcException(IpcErrorKind.BrokenPipe, "the outgoing direction was closed");
                }
                if (_aborted)
                {
                    throw IpcException.BrokenPipe();
                }
            }
            var count = Math.Min(data.Length, _settings.Chunk);
            SendReliably(Data, data[..count], null);
            return count;
        }
    }

    public void CloseWrite()
    {
        lock (_sending)
        {
            lock (_state)
            {
                if (_writeClosed || _aborted || _closed)
                {
                    return;
                }
                _writeClosed = true;
            }
            SendReliably(Eof, [], null);
        }
    }

    public void Shutdown()
    {
        lock (_state)
        {
            if (_closed)
            {
                return;
            }
            _closed = true;
            Monitor.PulseAll(_state);
        }
        IpcTransports.Trace($"{Kind} session hanging up");
        // Waits for a write in progress, which now fails quickly.
        lock (_sending)
        {
            bool end;
            lock (_state)
            {
                end = !_writeClosed && !_aborted;
                _writeClosed = true;
            }
            if (end)
            {
                try
                {
                    SendReliably(Eof, [], Deadline.After(Linger));
                }
                catch (Exception error) when (error is IOException or ObjectDisposedException or InvalidOperationException)
                {
                }
            }
            bool aborted;
            lock (_state)
            {
                aborted = _aborted;
            }
            if (!aborted)
            {
                for (var i = 0; i < 3; i++)
                {
                    TrySend(Frame(Abort, 0, []));
                }
            }
            lock (_state)
            {
                _stopping = true;
                Monitor.PulseAll(_state);
            }
        }
    }

    public void Dispose() => Shutdown();
}
