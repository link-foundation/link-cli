using System.Diagnostics.CodeAnalysis;
using System.Diagnostics;
using System.Net;
using System.Net.Sockets;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>A connected stream socket (<c>tcp</c> or <c>unix</c>) as a transport.</summary>
internal sealed class SocketTransport(Socket socket, string kind) : ITransport, IDisposable
{
    private readonly Lock _timeoutLock = new();
    private int _timeout = -1;

    public string Kind { get; } = kind;

    public int Read(Span<byte> buffer, TimeSpan? timeout)
    {
        var millis = timeout is { } value ? (int)Math.Clamp(Math.Ceiling(value.TotalMilliseconds), 1, int.MaxValue) : 0;
        lock (_timeoutLock)
        {
            if (_timeout != millis)
            {
                socket.ReceiveTimeout = millis;
                _timeout = millis;
            }
        }
        try
        {
            return socket.Receive(buffer);
        }
        catch (SocketException error) when (error.SocketErrorCode is SocketError.TimedOut or SocketError.WouldBlock)
        {
            throw IpcException.TimedOut();
        }
        // A hung-up socket ends like a closed one.
        catch (SocketException error) when (error.SocketErrorCode is SocketError.NotConnected or SocketError.ConnectionAborted
            or SocketError.Shutdown)
        {
            return 0;
        }
        catch (SocketException error)
        {
            throw IpcTransports.Wrap(error);
        }
        catch (ObjectDisposedException)
        {
            return 0;
        }
    }

    public int Write(ReadOnlySpan<byte> data)
    {
        try
        {
            return socket.Send(data);
        }
        catch (SocketException error)
        {
            throw error.SocketErrorCode is SocketError.Shutdown or SocketError.ConnectionAborted
                ? IpcException.BrokenPipe()
                : IpcTransports.Wrap(error);
        }
    }

    public void CloseWrite()
    {
        try
        {
            socket.Shutdown(SocketShutdown.Send);
        }
        catch (SocketException error) when (error.SocketErrorCode == SocketError.NotConnected)
        {
        }
        catch (SocketException error)
        {
            throw IpcTransports.Wrap(error);
        }
    }

    public void Shutdown()
    {
        try
        {
            socket.Shutdown(SocketShutdown.Both);
        }
        catch (Exception error) when (error is SocketException or ObjectDisposedException)
        {
        }
    }

    public void Dispose() => socket.Dispose();
}

/// <summary>The stream sockets <c>tcp</c> and <c>unix</c>.</summary>
internal static class NetTransports
{
    /// <summary>Splits <c>HOST:PORT</c> and resolves it, as Rust <c>ToSocketAddrs</c> does.</summary>
    public static IReadOnlyList<IPEndPoint> Resolve(string address)
    {
        var colon = address.LastIndexOf(':');
        if (colon < 0 || !ushort.TryParse(address[(colon + 1)..], out var port))
        {
            throw IpcException.Invalid($"invalid socket address `{address}`");
        }
        var host = address[..colon];
        if (host.StartsWith('[') && host.EndsWith(']'))
        {
            host = host[1..^1];
        }
        IPAddress[] addresses;
        if (IPAddress.TryParse(host, out var literal))
        {
            addresses = [literal];
        }
        else
        {
            try
            {
                addresses = Dns.GetHostAddresses(host);
            }
            catch (SocketException error)
            {
                throw IpcTransports.Wrap(error);
            }
        }
        if (addresses.Length == 0)
        {
            throw IpcException.Invalid($"`{address}` resolves to no address");
        }
        return [.. addresses.Select(ip => new IPEndPoint(ip, port))];
    }

    /// <summary>Wraps a connected TCP socket.</summary>
    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IpcStream TcpStream(Socket socket)
    {
        try
        {
            socket.NoDelay = true;
        }
        catch (SocketException error)
        {
            // The connection still works, only slower.
            IpcTransports.Trace($"could not disable Nagle's algorithm: {error.Message}");
        }
        return new IpcStream(new SocketTransport(socket, "tcp"));
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IpcStream ConnectTcp(Endpoint endpoint)
    {
        Exception? last = null;
        foreach (var address in Resolve(endpoint.Address))
        {
            var socket = new Socket(address.AddressFamily, SocketType.Stream, ProtocolType.Tcp);
            try
            {
                if (endpoint.Option("timeout") is not null)
                {
                    using var cancel = new CancellationTokenSource(endpoint.Timeout);
                    try
                    {
                        socket.ConnectAsync(address, cancel.Token).AsTask().GetAwaiter().GetResult();
                    }
                    catch (OperationCanceledException)
                    {
                        throw new IpcException(IpcErrorKind.TimedOut, $"connecting to {address} timed out");
                    }
                }
                else
                {
                    socket.Connect(address);
                }
                return TcpStream(socket);
            }
            catch (Exception error) when (error is SocketException or IpcException)
            {
                socket.Dispose();
                last = error;
            }
        }
        throw IpcTransports.Wrap(last!);
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IIpcListener ListenTcp(Endpoint endpoint)
    {
        Exception? last = null;
        foreach (var address in Resolve(endpoint.Address))
        {
            var listener = new TcpListener(address);
            try
            {
                listener.Start();
                return new TcpIpcListener(listener);
            }
            catch (SocketException error)
            {
                listener.Stop();
                last = error;
            }
        }
        throw IpcTransports.Wrap(last!);
    }

    // ------------------------------------------------------------ unix

    private static UnixDomainSocketEndPoint UnixAddress(string name)
    {
        if (name.StartsWith('@'))
        {
            if (OperatingSystem.IsWindows())
            {
                throw new IpcException(IpcErrorKind.Unsupported, "abstract `unix:@name` sockets are Linux-only");
            }
            if (!OperatingSystem.IsLinux())
            {
                throw new IpcException(IpcErrorKind.Unsupported, "abstract `unix:@NAME` sockets exist only on Linux");
            }
            return new UnixDomainSocketEndPoint("\0" + name[1..]);
        }
        try
        {
            return new UnixDomainSocketEndPoint(name);
        }
        catch (ArgumentException error)
        {
            throw new IpcException(IpcErrorKind.InvalidInput,
                $"`{name}` is not a valid socket path (at most 107 bytes, no NUL)", error);
        }
    }

    private static Socket ConnectName(string name)
    {
        var address = UnixAddress(name);
        var socket = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
        try
        {
            socket.Connect(address);
            return socket;
        }
        catch (SocketException error)
        {
            socket.Dispose();
            throw error.SocketErrorCode == SocketError.AddressNotAvailable
                ? new IpcException(IpcErrorKind.NotFound, error.Message, error)
                : IpcTransports.Wrap(error);
        }
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IpcStream ConnectUnix(Endpoint endpoint) =>
        new(new SocketTransport(ConnectName(endpoint.Address), "unix"));

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IIpcListener ListenUnix(Endpoint endpoint)
    {
        var name = endpoint.Address;
        var address = UnixAddress(name);
        string? path = null;
        if (!name.StartsWith('@'))
        {
            path = name;
            if (File.Exists(path) || Directory.Exists(path))
            {
                try
                {
                    using var probe = ConnectName(name);
                }
                catch (IpcException)
                {
                    IpcTransports.Trace($"unix removes the stale socket `{name}`");
                    File.Delete(path);
                    goto bind;
                }
                throw new IpcException(IpcErrorKind.AddrInUse, $"a server already listens on `{name}`");
            }
        }
    bind:
        var socket = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
        try
        {
            socket.Bind(address);
            socket.Listen(128);
        }
        catch (SocketException error)
        {
            socket.Dispose();
            throw IpcTransports.Wrap(error);
        }
        return new UnixIpcListener(socket, name, path);
    }

    private sealed class UnixIpcListener(Socket socket, string name, string? path) : IIpcListener
    {
        private volatile bool _closed;

        [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
        public IpcStream? Accept()
        {
            while (true)
            {
                if (_closed)
                {
                    return null;
                }
                Socket accepted;
                try
                {
                    accepted = socket.Accept();
                }
                catch (SocketException error) when (error.SocketErrorCode is SocketError.Interrupted or SocketError.ConnectionAborted)
                {
                    continue;
                }
                catch (SocketException error)
                {
                    throw IpcTransports.Wrap(error);
                }
                if (_closed)
                {
                    accepted.Dispose();
                    return null;
                }
                IpcTransports.Trace($"unix accepted on {name}");
                return new IpcStream(new SocketTransport(accepted, "unix"));
            }
        }

        public string Endpoint => $"unix:{name}";

        public Action Closer() => () =>
        {
            if (!_closed)
            {
                _closed = true;
                try
                {
                    ConnectName(name).Dispose();
                }
                catch (IpcException)
                {
                }
            }
        };

        public void Dispose()
        {
            _closed = true;
            socket.Dispose();
            if (path is not null)
            {
                RendezvousFiles.TryDelete(path);
            }
        }
    }
}

/// <summary>A TCP listener; its endpoint is the bare <c>HOST:PORT</c>, as the first protocol versions printed it.</summary>
internal sealed class TcpIpcListener : IIpcListener
{
    private readonly TcpListener _listener;
    private readonly IPEndPoint _address;
    private volatile bool _closed;

    public TcpIpcListener(TcpListener listener)
    {
        _listener = listener;
        _address = (IPEndPoint)listener.LocalEndpoint;
    }

    public IpcStream? Accept()
    {
        while (true)
        {
            if (_closed)
            {
                return null;
            }
            Socket socket;
            try
            {
                socket = _listener.AcceptSocket();
            }
            catch (SocketException error) when (error.SocketErrorCode is SocketError.Interrupted or SocketError.ConnectionAborted
                or SocketError.ConnectionReset)
            {
                continue;
            }
            catch (SocketException error)
            {
                throw IpcTransports.Wrap(error);
            }
            catch (ObjectDisposedException)
            {
                return null;
            }
            if (_closed)
            {
                socket.Dispose();
                return null;
            }
            IpcTransports.Trace($"tcp accepted {socket.RemoteEndPoint}");
            return NetTransports.TcpStream(socket);
        }
    }

    public string Endpoint => _address.ToString();

    public IPEndPoint? LocalEndPoint => _address;

    /// <summary>Where to connect to reach a socket bound to the listening address.</summary>
    private IPEndPoint Reachable()
    {
        if (_address.Address.Equals(IPAddress.Any))
        {
            return new IPEndPoint(IPAddress.Loopback, _address.Port);
        }
        if (_address.Address.Equals(IPAddress.IPv6Any))
        {
            return new IPEndPoint(IPAddress.IPv6Loopback, _address.Port);
        }
        return _address;
    }

    public Action Closer() => () =>
    {
        if (_closed)
        {
            return;
        }
        _closed = true;
        using var socket = new Socket(_address.AddressFamily, SocketType.Stream, ProtocolType.Tcp);
        try
        {
            using var cancel = new CancellationTokenSource(TimeSpan.FromSeconds(1));
            socket.ConnectAsync(Reachable(), cancel.Token).AsTask().GetAwaiter().GetResult();
        }
        catch (Exception error) when (error is SocketException or OperationCanceledException)
        {
        }
    };

    public void Dispose()
    {
        _closed = true;
        _listener.Stop();
    }
}

/// <summary>The <c>udp</c> kind: a reliable session per client over loopback datagrams.</summary>
internal static class UdpTransports
{
    private static readonly TimeSpan WelcomeKept = TimeSpan.FromSeconds(30);

    internal static bool Ignorable(SocketException error) => error.SocketErrorCode is SocketError.ConnectionRefused
        or SocketError.ConnectionReset or SocketError.WouldBlock or SocketError.TimedOut or SocketError.Interrupted;

    /// <summary>A connected UDP socket.</summary>
    private sealed class UdpLink(Socket socket) : IDatagrams
    {
        private readonly byte[] _buffer = new byte[65536];
        private int _timeout = -1;

        public bool Send(ReadOnlySpan<byte> frame)
        {
            try
            {
                socket.Send(frame);
                return true;
            }
            catch (SocketException error) when (Ignorable(error))
            {
                return false;
            }
        }

        public byte[]? Receive(TimeSpan timeout)
        {
            var millis = (int)Math.Clamp(Math.Ceiling(timeout.TotalMilliseconds), 1, int.MaxValue);
            if (_timeout != millis)
            {
                socket.ReceiveTimeout = millis;
                _timeout = millis;
            }
            try
            {
                var count = socket.Receive(_buffer);
                return _buffer.AsSpan(0, count).ToArray();
            }
            catch (SocketException error) when (Ignorable(error))
            {
                return null;
            }
        }

        public void Close() => socket.Dispose();
    }

    private static DatagramSettings Settings(Endpoint endpoint, int chunk)
    {
        var minimum = TimeSpan.FromMilliseconds(1);
        var heartbeat = endpoint.Millis("heartbeat", 1000);
        var dead = endpoint.Millis("dead", 10_000);
        return new DatagramSettings(chunk, endpoint.Millis("rto", 20), heartbeat < minimum ? minimum : heartbeat,
            dead < minimum ? minimum : dead);
    }

    private static byte[] HelloPayload(ReadOnlySpan<byte> token, uint chunk)
    {
        var payload = new byte[20];
        token.CopyTo(payload);
        System.Buffers.Binary.BinaryPrimitives.WriteUInt32LittleEndian(payload.AsSpan(16), chunk);
        return payload;
    }

    private static bool TokenAndChunk(ReadOnlySpan<byte> payload, out byte[] token, out uint chunk)
    {
        if (payload.Length != 20)
        {
            (token, chunk) = ([], 0);
            return false;
        }
        token = payload[..16].ToArray();
        chunk = System.Buffers.Binary.BinaryPrimitives.ReadUInt32LittleEndian(payload[16..]);
        return true;
    }

    private static IPEndPoint Unspecified(IPEndPoint address) =>
        new(address.AddressFamily == AddressFamily.InterNetworkV6 ? IPAddress.IPv6Any : IPAddress.Any, 0);

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IpcStream Connect(Endpoint endpoint)
    {
        var server = NetTransports.Resolve(endpoint.Address)[0];
        var socket = new Socket(server.AddressFamily, SocketType.Dgram, ProtocolType.Udp);
        try
        {
            socket.Bind(Unspecified(server));
            var ownChunk = endpoint.Number("chunk", 1200, 16, 65000);
            var token = IpcTransports.RandomToken();
            var hello = DatagramSession.Frame(DatagramSession.Hello, 0, HelloPayload(token, (uint)ownChunk));
            var deadline = Deadline.After(endpoint.Timeout);
            socket.ReceiveTimeout = 100;
            var buffer = new byte[65536];
            var nextHello = Stopwatch.GetTimestamp();
            while (true)
            {
                if (deadline.Passed)
                {
                    throw new IpcException(IpcErrorKind.TimedOut, $"no `udp` listener answered at {server}");
                }
                if (Stopwatch.GetTimestamp() >= nextHello)
                {
                    try
                    {
                        socket.SendTo(hello, server);
                    }
                    catch (SocketException error) when (Ignorable(error))
                    {
                    }
                    nextHello = Stopwatch.GetTimestamp() + Stopwatch.Frequency / 10;
                }
                EndPoint source = Unspecified(server);
                int count;
                try
                {
                    count = socket.ReceiveFrom(buffer, ref source);
                }
                catch (SocketException error) when (Ignorable(error))
                {
                    continue;
                }
                if (!DatagramSession.TryParse(buffer.AsSpan(0, count), out var kind, out _, out var payload)
                    || kind != DatagramSession.Welcome
                    || !TokenAndChunk(payload, out var welcomeToken, out var chunk)
                    || !welcomeToken.AsSpan().SequenceEqual(token))
                {
                    continue;
                }
                socket.Connect(source);
                var agreed = DatagramSession.AgreeChunk(ownChunk, chunk);
                IpcTransports.Trace($"udp connected to {source} through {server}, chunk {agreed}");
                return new IpcStream(new DatagramSession("udp", new UdpLink(socket), Settings(endpoint, agreed)));
            }
        }
        catch (SocketException error)
        {
            socket.Dispose();
            throw IpcTransports.Wrap(error);
        }
        catch
        {
            socket.Dispose();
            throw;
        }
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IIpcListener Listen(Endpoint endpoint)
    {
        var ownChunk = endpoint.Number("chunk", 1200, 16, 65000);
        Exception? last = null;
        foreach (var address in NetTransports.Resolve(endpoint.Address))
        {
            var socket = new Socket(address.AddressFamily, SocketType.Dgram, ProtocolType.Udp);
            try
            {
                socket.Bind(address);
                socket.ReceiveTimeout = 100;
                return new UdpIpcListener(socket, endpoint, ownChunk);
            }
            catch (SocketException error)
            {
                socket.Dispose();
                last = error;
            }
        }
        throw IpcTransports.Wrap(last!);
    }

    private sealed class UdpIpcListener(Socket socket, Endpoint endpoint, ulong ownChunk) : IIpcListener
    {
        private readonly IPEndPoint _address = (IPEndPoint)socket.LocalEndPoint!;
        /// <summary>(source, token) → the session socket, the WELCOME sent from it, and when.</summary>
        private readonly Dictionary<string, (Socket Socket, byte[] Welcome, long Created)> _sessions = [];
        private volatile bool _closed;

        [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
        public IpcStream? Accept()
        {
            var buffer = new byte[65536];
            while (true)
            {
                if (_closed)
                {
                    return null;
                }
                EndPoint from = new IPEndPoint(_address.AddressFamily == AddressFamily.InterNetworkV6 ? IPAddress.IPv6Any : IPAddress.Any, 0);
                int count;
                try
                {
                    count = socket.ReceiveFrom(buffer, ref from);
                }
                catch (SocketException error) when (Ignorable(error))
                {
                    continue;
                }
                catch (SocketException error)
                {
                    throw IpcTransports.Wrap(error);
                }
                catch (ObjectDisposedException)
                {
                    return null;
                }
                var source = (IPEndPoint)from;
                foreach (var stale in _sessions.Where(entry => Stopwatch.GetElapsedTime(entry.Value.Created) >= WelcomeKept)
                    .Select(entry => entry.Key).ToList())
                {
                    _sessions.Remove(stale);
                }
                if (!DatagramSession.TryParse(buffer.AsSpan(0, count), out var kind, out _, out var payload)
                    || kind != DatagramSession.Hello
                    || !TokenAndChunk(payload, out var token, out var chunk))
                {
                    continue;
                }
                var key = $"{source}/{Convert.ToHexString(token)}";
                if (_sessions.TryGetValue(key, out var existing))
                {
                    try
                    {
                        existing.Socket.Send(existing.Welcome);
                    }
                    catch (Exception error) when (error is SocketException or ObjectDisposedException)
                    {
                    }
                    continue;
                }
                var session = new Socket(_address.AddressFamily, SocketType.Dgram, ProtocolType.Udp);
                try
                {
                    session.Bind(new IPEndPoint(_address.Address, 0));
                    session.Connect(source);
                }
                catch (SocketException error)
                {
                    session.Dispose();
                    throw IpcTransports.Wrap(error);
                }
                var agreed = DatagramSession.AgreeChunk(ownChunk, chunk);
                var welcome = DatagramSession.Frame(DatagramSession.Welcome, 0, HelloPayload(token, (uint)agreed));
                try
                {
                    session.Send(welcome);
                }
                catch (SocketException)
                {
                }
                _sessions[key] = (session, welcome, Stopwatch.GetTimestamp());
                IpcTransports.Trace($"udp accepted {source} on {session.LocalEndPoint}, chunk {agreed}");
                return new IpcStream(new DatagramSession("udp", new UdpLink(session), Settings(endpoint, agreed)));
            }
        }

        public string Endpoint => $"udp:{_address}";

        public Action Closer() => () => _closed = true;

        public void Dispose()
        {
            _closed = true;
            socket.Dispose();
        }
    }
}
