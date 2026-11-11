using System.Diagnostics.CodeAnalysis;
using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>
/// Interchangeable inter-process communication transports (issue #140): every IPC mechanism of the operating
/// system as an <see cref="IpcStream"/>, chosen by an endpoint string, so the mechanism is a decorator layer under
/// the LiNo text and binary protocols.
/// </summary>
/// <remarks>
/// <para>The wire format of every kind is specified in <c>docs/protocol/ipc-transports-v1.md</c>; the Rust and
/// JavaScript implementations follow the same document, so any of them talks to any other.</para>
/// <list type="table">
/// <listheader><term>Endpoint</term><description>Mechanism</description></listheader>
/// <item><term><c>host:port</c>, <c>tcp:host:port</c></term><description>TCP/IP</description></item>
/// <item><term><c>udp:host:port</c></term><description>UDP datagrams with a reliable session</description></item>
/// <item><term><c>unix:PATH</c>, <c>unix:@NAME</c></term><description>Unix domain stream socket</description></item>
/// <item><term><c>pipe:NAME</c></term><description>Windows named pipe; on POSIX the same as <c>fifo</c></description></item>
/// <item><term><c>fifo:DIR</c></term><description>two POSIX named pipes</description></item>
/// <item><term><c>stdio</c>, <c>exec:COMMAND</c></term><description>anonymous pipes of standard input and output</description></item>
/// <item><term><c>shm:DIR</c>, <c>mmap:DIR</c>, <c>file:DIR</c></term><description>shared memory, memory-mapped file, file on disk</description></item>
/// <item><term><c>signal:DIR</c></term><description>memory-mapped ring announced by OS signals (Windows: events)</description></item>
/// <item><term><c>mq:DIR</c>, <c>sysvmq:DIR</c></term><description>POSIX and System V message queues</description></item>
/// <item><term><c>mailslot:DIR</c>, <c>copydata:DIR</c>, <c>alpc:DIR</c></term><description>Windows mailslots, <c>WM_COPYDATA</c>, ALPC</description></item>
/// </list>
/// </remarks>
public static class IpcTransports
{
    /// <summary>Why the factories here keep what they create: the stream or listener returned owns it, and failures dispose it.</summary>
    internal const string OwnershipTransferred =
        "The created object is owned by the returned stream or listener, and failure paths dispose it.";

    /// <summary>Every built-in endpoint kind, in documentation order.</summary>
    public static IReadOnlyList<string> Kinds { get; } =
    [
        "tcp", "udp", "unix", "pipe", "fifo", "stdio", "exec", "shm", "mmap", "file", "signal", "mq", "sysvmq",
        "mailslot", "copydata", "alpc",
    ];

    private sealed record Registered(Func<Endpoint, IpcStream>? Connect, Func<Endpoint, IIpcListener>? Listen);

    private static readonly Dictionary<string, Registered> Registry = new(StringComparer.Ordinal);
    private static readonly Lock RegistryLock = new();

    private static Registered? Lookup(string kind)
    {
        lock (RegistryLock)
        {
            return Registry.GetValueOrDefault(kind.ToLowerInvariant());
        }
    }

    /// <summary>Whether <paramref name="kind"/> was added with <see cref="Register"/>.</summary>
    public static bool IsRegistered(string kind) => Lookup(kind) is not null;

    /// <summary>
    /// Adds a custom endpoint kind, such as a transport over a device driver. The kind is lowercase ASCII
    /// letters, digits and <c>-</c>; built-in kinds cannot be replaced, and registering a kind again replaces the
    /// earlier one.
    /// </summary>
    public static void Register(string kind, Func<Endpoint, IpcStream>? connect, Func<Endpoint, IIpcListener>? listen)
    {
        ArgumentNullException.ThrowIfNull(kind);
        var lower = kind.ToLowerInvariant();
        if (lower.Length == 0 || !lower.All(c => c is >= 'a' and <= 'z' or >= '0' and <= '9' or '-'))
        {
            throw IpcException.Invalid($"invalid endpoint kind `{lower}`");
        }
        if (Kinds.Contains(lower))
        {
            throw new IpcException(IpcErrorKind.AlreadyExists, $"`{lower}` is a built-in endpoint kind");
        }
        lock (RegistryLock)
        {
            Registry[lower] = new Registered(connect, listen);
        }
    }

    /// <summary>Connects to <paramref name="endpoint"/> (see <see cref="Ipc.Endpoint"/>).</summary>
    public static IpcStream Connect(string endpoint) => Connect(Endpoint.Parse(endpoint));

    /// <summary>Connects to a parsed endpoint.</summary>
    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IpcStream Connect(Endpoint endpoint)
    {
        ArgumentNullException.ThrowIfNull(endpoint);
        Trace($"connect {endpoint}");
        try
        {
            var stream = Lookup(endpoint.Kind) switch
            {
                { Connect: { } connect } => connect(endpoint),
                not null => throw IpcException.Unsupported(endpoint.Kind, "connecting"),
                null => BuiltinConnect(endpoint),
            };
            Trace($"connected {endpoint}");
            return stream;
        }
        catch (Exception error) when (error is IOException or SocketException or UnauthorizedAccessException)
        {
            Trace($"connect {endpoint} failed: {error.Message}");
            throw Wrap(error);
        }
    }

    /// <summary>Listens on <paramref name="endpoint"/> (see <see cref="Ipc.Endpoint"/>).</summary>
    public static IIpcListener Listen(string endpoint) => Listen(Endpoint.Parse(endpoint));

    /// <summary>Listens on a parsed endpoint.</summary>
    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IIpcListener Listen(Endpoint endpoint)
    {
        ArgumentNullException.ThrowIfNull(endpoint);
        IIpcListener listener;
        try
        {
            listener = Lookup(endpoint.Kind) switch
            {
                { Listen: { } listen } => listen(endpoint),
                not null => throw IpcException.Unsupported(endpoint.Kind, "listening"),
                null => BuiltinListen(endpoint),
            };
        }
        catch (Exception error) when (error is IOException or SocketException or UnauthorizedAccessException)
        {
            throw Wrap(error);
        }
        Trace($"listening on {listener.Endpoint}");
        return listener;
    }

    /// <summary>Listens on every endpoint at once; one listener is returned as it is.</summary>
    public static IIpcListener ListenAll(IEnumerable<string> endpoints)
    {
        ArgumentNullException.ThrowIfNull(endpoints);
        var listeners = new List<IIpcListener>();
        try
        {
            foreach (var endpoint in endpoints)
            {
                listeners.Add(Listen(endpoint));
            }
        }
        catch
        {
            foreach (var listener in listeners)
            {
                listener.Dispose();
            }
            throw;
        }
        return listeners.Count switch
        {
            0 => throw IpcException.Invalid("no endpoint to listen on"),
            1 => listeners[0],
            _ => new MultiListener(listeners),
        };
    }

    /// <summary>Wraps a connected TCP socket (Nagle's algorithm is turned off).</summary>
    public static IpcStream FromTcpSocket(Socket socket) => NetTransports.TcpStream(socket);

    /// <summary>Wraps an already bound and listening TCP listener.</summary>
    public static IIpcListener FromTcpListener(TcpListener listener) => new TcpIpcListener(listener);

    /// <summary>Whether this build can connect to (<paramref name="listen"/> false) or listen on the kind here.</summary>
    public static bool Supports(string kind, bool listen)
    {
        ArgumentNullException.ThrowIfNull(kind);
        if (Lookup(kind) is { } registered)
        {
            return listen ? registered.Listen is not null : registered.Connect is not null;
        }
        var posixOrWindows = OperatingSystem.IsWindows() || IsPosix;
        return kind.ToLowerInvariant() switch
        {
            "tcp" or "udp" or "mmap" or "file" or "shm" or "signal" or "pipe" or "unix" => posixOrWindows,
            "stdio" => listen,
            "exec" => !listen,
            "fifo" => IsPosix,
            "sysvmq" => OperatingSystem.IsLinux() || OperatingSystem.IsMacOS(),
            "mq" => OperatingSystem.IsLinux(),
            "mailslot" or "copydata" => OperatingSystem.IsWindows(),
            "alpc" => OperatingSystem.IsWindows() && Environment.Is64BitProcess,
            _ => false,
        };
    }

    internal static bool IsPosix => OperatingSystem.IsLinux() || OperatingSystem.IsMacOS() || OperatingSystem.IsFreeBSD();

    private static IpcStream BuiltinConnect(Endpoint endpoint)
    {
        switch (endpoint.Kind)
        {
            case "tcp": return NetTransports.ConnectTcp(endpoint);
            case "udp": return UdpTransports.Connect(endpoint);
            case "exec": return StdioTransports.ConnectExec(endpoint);
            case "stdio":
                throw new IpcException(IpcErrorKind.Unsupported,
                    "`stdio` can only be listened on; connect with `exec:COMMAND` instead");
            case "mmap" or "file" or "shm" or "signal": return RingTransports.Connect(endpoint);
            case "unix": return NetTransports.ConnectUnix(endpoint);
        }
        if (OperatingSystem.IsWindows())
        {
            switch (endpoint.Kind)
            {
                case "pipe": return WindowsPipe.Connect(endpoint);
                case "mailslot": return WindowsMailslot.Connect(endpoint);
                case "copydata": return WindowsCopyData.Connect(endpoint);
                case "alpc" when Environment.Is64BitProcess: return WindowsAlpc.Connect(endpoint);
            }
        }
        else if (IsPosix)
        {
            switch (endpoint.Kind)
            {
                case "pipe" or "fifo": return PosixFifo.Connect(endpoint);
                case "sysvmq" when OperatingSystem.IsLinux() || OperatingSystem.IsMacOS(): return SysVMessageQueue.Connect(endpoint);
                case "mq" when OperatingSystem.IsLinux(): return PosixMessageQueue.Connect(endpoint);
            }
        }
        throw IpcException.Unsupported(endpoint.Kind, "connecting");
    }

    private static IIpcListener BuiltinListen(Endpoint endpoint)
    {
        switch (endpoint.Kind)
        {
            case "tcp": return NetTransports.ListenTcp(endpoint);
            case "udp": return UdpTransports.Listen(endpoint);
            case "stdio": return StdioTransports.ListenStdio(endpoint);
            case "exec":
                throw new IpcException(IpcErrorKind.Unsupported,
                    "`exec` can only be connected to; listen on `stdio` in the started command");
            case "mmap" or "file" or "shm" or "signal": return RingTransports.Listen(endpoint);
            case "unix": return NetTransports.ListenUnix(endpoint);
        }
        if (OperatingSystem.IsWindows())
        {
            switch (endpoint.Kind)
            {
                case "pipe": return WindowsPipe.Listen(endpoint);
                case "mailslot": return WindowsMailslot.Listen(endpoint);
                case "copydata": return WindowsCopyData.Listen(endpoint);
                case "alpc" when Environment.Is64BitProcess: return WindowsAlpc.Listen(endpoint);
            }
        }
        else if (IsPosix)
        {
            switch (endpoint.Kind)
            {
                case "pipe" or "fifo": return PosixFifo.Listen(endpoint);
                case "sysvmq" when OperatingSystem.IsLinux() || OperatingSystem.IsMacOS(): return SysVMessageQueue.Listen(endpoint);
                case "mq" when OperatingSystem.IsLinux(): return PosixMessageQueue.Listen(endpoint);
            }
        }
        throw IpcException.Unsupported(endpoint.Kind, "listening");
    }

    /// <summary>Gives every transport failure the <see cref="IpcException"/> type, keeping its message.</summary>
    internal static IpcException Wrap(Exception error) => error switch
    {
        IpcException ipc => ipc,
        SocketException socket => new IpcException(SocketKind(socket.SocketErrorCode), socket.Message, socket),
        FileNotFoundException or DirectoryNotFoundException => new IpcException(IpcErrorKind.NotFound, error.Message, error),
        UnauthorizedAccessException => new IpcException(IpcErrorKind.PermissionDenied, error.Message, error),
        _ => new IpcException(IpcErrorKind.Other, error.Message, error),
    };

    internal static IpcErrorKind SocketKind(SocketError error) => error switch
    {
        SocketError.ConnectionRefused => IpcErrorKind.ConnectionRefused,
        SocketError.ConnectionReset or SocketError.ConnectionAborted or SocketError.Shutdown => IpcErrorKind.ConnectionReset,
        SocketError.AddressAlreadyInUse => IpcErrorKind.AddrInUse,
        SocketError.TimedOut or SocketError.WouldBlock => IpcErrorKind.TimedOut,
        SocketError.AddressNotAvailable or SocketError.HostNotFound => IpcErrorKind.NotFound,
        _ => IpcErrorKind.Other,
    };

    /// <summary>
    /// Copies both directions between <paramref name="a"/> and <paramref name="b"/> until both ended, passing the
    /// end of each direction on with <see cref="IpcStream.CloseWrite"/>. Returns the bytes copied from a to b and
    /// from b to a. This stacks transports: a relay never parses the protocol.
    /// </summary>
    public static (long Forward, long Backward) Forward(IpcStream a, IpcStream b)
    {
        ArgumentNullException.ThrowIfNull(a);
        ArgumentNullException.ThrowIfNull(b);
        using var a2 = a.Clone();
        using var b2 = b.Clone();
        long backward = 0;
        Exception? backwardError = null;
        var thread = new Thread(() =>
        {
            try
            {
                backward = Pump(b2, a2);
            }
            catch (Exception error) when (error is IOException or ObjectDisposedException)
            {
                backwardError = error;
            }
        }) { IsBackground = true, Name = "clink-relay" };
        thread.Start();
        long forward = 0;
        Exception? forwardError = null;
        try
        {
            forward = Pump(a, b);
        }
        catch (Exception error) when (error is IOException or ObjectDisposedException)
        {
            forwardError = error;
        }
        thread.Join();
        a.Shutdown();
        b.Shutdown();
        if ((forwardError ?? backwardError) is { } failure)
        {
            throw Wrap(failure);
        }
        return (forward, backward);
    }

    private static long Pump(IpcStream from, IpcStream to)
    {
        var buffer = new byte[64 * 1024];
        long total = 0;
        while (true)
        {
            int read;
            try
            {
                read = from.Read(buffer);
            }
            catch (Exception error) when (error is IOException or ObjectDisposedException)
            {
                Trace($"relay {from.Kind} -> {to.Kind}: {error.Message}");
                // The other direction cannot go on either.
                to.Shutdown();
                from.Shutdown();
                throw;
            }
            if (read == 0)
            {
                break;
            }
            try
            {
                to.Write(buffer.AsSpan(0, read));
                to.Flush();
            }
            catch (Exception error) when (error is IOException or ObjectDisposedException)
            {
                Trace($"relay {from.Kind} -> {to.Kind}: {error.Message}");
                from.Shutdown();
                to.Shutdown();
                throw;
            }
            total += read;
        }
        Trace($"relay {from.Kind} -> {to.Kind}: end after {total} bytes");
        try
        {
            to.CloseWrite();
        }
        catch (Exception error) when (error is IOException or ObjectDisposedException)
        {
        }
        return total;
    }

    /// <summary>
    /// Accepts connections on <paramref name="listener"/> and forwards each one to a new connection to
    /// <paramref name="target"/> (see <see cref="Forward"/>) until the listener is closed or exhausted, then waits
    /// for the forwarded connections to end.
    /// </summary>
    public static void Relay(IIpcListener listener, Endpoint target)
    {
        ArgumentNullException.ThrowIfNull(listener);
        ArgumentNullException.ThrowIfNull(target);
        var connections = new List<Thread>();
        while (true)
        {
            IpcStream? stream;
            try
            {
                stream = listener.Accept();
            }
            catch (Exception error) when (error is IOException or SocketException)
            {
                Trace($"relay accept on {listener.Endpoint} failed: {error.Message}");
                // A failing listener must not spin.
                Thread.Sleep(10);
                continue;
            }
            if (stream is null)
            {
                break;
            }
            var incoming = stream;
            var connection = new Thread(() => RelayOne(incoming, target)) { IsBackground = true, Name = "clink-relay" };
            connection.Start();
            connections.Add(connection);
            connections.RemoveAll(thread => !thread.IsAlive);
        }
        foreach (var connection in connections)
        {
            connection.Join();
        }
    }

    private static void RelayOne(IpcStream incoming, Endpoint target)
    {
        using (incoming)
        {
            IpcStream outgoing;
            try
            {
                outgoing = Connect(target);
            }
            catch (IOException error)
            {
                DiagnosticOutput.WriteErrorLine($"clink: cannot forward to {target}: {error.Message}");
                incoming.Shutdown();
                return;
            }
            using (outgoing)
            {
                try
                {
                    Forward(incoming, outgoing);
                }
                catch (IOException error)
                {
                    Trace($"relay to {target} ended: {error.Message}");
                }
            }
        }
    }

    private const int TraceUnset = 0;
    private const int TraceOff = 1;
    private const int TraceOn = 2;
    private static int _trace = TraceUnset;

    /// <summary>
    /// Turns the <c>[ipc]</c> transport log on standard error on or off. It starts on when the environment
    /// variable <c>CLINK_IPC_TRACE</c> is <c>1</c>.
    /// </summary>
    public static void SetTrace(bool enabled) => Volatile.Write(ref _trace, enabled ? TraceOn : TraceOff);

    /// <summary>Whether the <c>[ipc]</c> transport log is on.</summary>
    public static bool TraceEnabled
    {
        get
        {
            switch (Volatile.Read(ref _trace))
            {
                case TraceOn: return true;
                case TraceOff: return false;
                default:
                    var enabled = Environment.GetEnvironmentVariable("CLINK_IPC_TRACE") == "1";
                    SetTrace(enabled);
                    return enabled;
            }
        }
    }

    internal static void Trace(string message)
    {
        if (TraceEnabled)
        {
            DiagnosticOutput.WriteErrorLine($"[ipc] {message}");
        }
    }

    /// <summary>A fresh identifier of 20 lowercase hexadecimal digits.</summary>
    internal static string RandomId()
    {
        Span<byte> bytes = stackalloc byte[16];
        RandomNumberGenerator.Fill(bytes);
        var a = BitConverter.ToUInt64(bytes);
        var b = BitConverter.ToUInt64(bytes[8..]);
        return $"{a:x16}{b & 0xffff:x4}";
    }

    /// <summary>Sixteen bytes nobody else can predict.</summary>
    internal static byte[] RandomToken() => RandomNumberGenerator.GetBytes(16);

    /// <summary>The identifier of this process.</summary>
    internal static uint ProcessId => (uint)Environment.ProcessId;

    /// <summary>Whether the process <paramref name="pid"/> is still running.</summary>
    internal static bool ProcessAlive(uint pid)
    {
        if (OperatingSystem.IsWindows())
        {
            return WindowsNative.ProcessAlive(pid);
        }
        if (IsPosix)
        {
            return PosixNative.ProcessAlive(pid);
        }
        try
        {
            using var process = Process.GetProcessById((int)pid);
            return !process.HasExited;
        }
        catch (ArgumentException)
        {
            return false;
        }
    }
}

/// <summary>Watches whether a peer process is alive, checking it at most every 100 ms.</summary>
internal sealed class PeerWatch(uint pid)
{
    private readonly Lock _lock = new();
    private long _checked = Stopwatch.GetTimestamp();
    private bool _alive = true;

    public uint Pid { get; } = pid;

    public bool Alive
    {
        get
        {
            lock (_lock)
            {
                if (_alive && Stopwatch.GetElapsedTime(_checked) >= TimeSpan.FromMilliseconds(100))
                {
                    _alive = IpcTransports.ProcessAlive(Pid);
                    _checked = Stopwatch.GetTimestamp();
                }
                return _alive;
            }
        }
    }
}

/// <summary>A point in time measured on the monotonic clock.</summary>
internal readonly record struct Deadline(long Timestamp)
{
    public static Deadline After(TimeSpan span) =>
        new(Stopwatch.GetTimestamp() + (long)(span.TotalSeconds * Stopwatch.Frequency));

    public static Deadline? After(TimeSpan? span) => span is { } value ? After(value) : null;

    public bool Passed => Stopwatch.GetTimestamp() >= Timestamp;

    public TimeSpan Remaining
    {
        get
        {
            var left = Timestamp - Stopwatch.GetTimestamp();
            return left <= 0 ? TimeSpan.Zero : TimeSpan.FromSeconds((double)left / Stopwatch.Frequency);
        }
    }
}
