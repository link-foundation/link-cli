using System.Diagnostics.CodeAnalysis;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>The libc calls of FIFOs and message queues.</summary>
internal static partial class PosixNative
{
    public const short PollIn = 1;
    public const short PollOut = 4;

    [StructLayout(LayoutKind.Sequential)]
    public struct PollFd
    {
        public int Fd;
        public short Events;
        public short Revents;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct TimeSpec
    {
        public nint Seconds;
        public nint Nanoseconds;
    }

    /// <summary>Linux <c>struct mq_attr</c>: four <c>long</c> fields and four reserved ones.</summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct MqAttr
    {
        public nint Flags;
        public nint MaxMessages;
        public nint MessageSize;
        public nint CurrentMessages;
        public nint Reserved0;
        public nint Reserved1;
        public nint Reserved2;
        public nint Reserved3;
    }

    [LibraryImport(Libc, EntryPoint = "mkfifo", SetLastError = true, StringMarshalling = StringMarshalling.Utf8)]
    private static partial int MkfifoNative(string path, uint mode);

    [LibraryImport(Libc, EntryPoint = "open", SetLastError = true, StringMarshalling = StringMarshalling.Utf8)]
    private static partial int OpenNative(string path, int flags);

    [LibraryImport(Libc, EntryPoint = "poll", SetLastError = true)]
    private static partial int PollNative(ref PollFd entry, nuint count, int timeout);

    [LibraryImport(Libc, EntryPoint = "read", SetLastError = true)]
    private static unsafe partial nint ReadNative(int fd, byte* buffer, nuint count);

    [LibraryImport(Libc, EntryPoint = "write", SetLastError = true)]
    private static unsafe partial nint WriteNative(int fd, byte* buffer, nuint count);

    [LibraryImport(Libc, EntryPoint = "clock_gettime", SetLastError = true)]
    private static partial int ClockGettime(int clock, out TimeSpec now);

    [LibraryImport(Libc, EntryPoint = "mq_open", SetLastError = true, StringMarshalling = StringMarshalling.Utf8)]
    private static partial int MqOpenCreate(string name, int flags, uint mode, ref MqAttr attributes);

    [LibraryImport(Libc, EntryPoint = "mq_open", SetLastError = true, StringMarshalling = StringMarshalling.Utf8)]
    private static partial int MqOpenExisting(string name, int flags);

    [LibraryImport(Libc, EntryPoint = "mq_close", SetLastError = true)]
    public static partial int MqClose(int queue);

    [LibraryImport(Libc, EntryPoint = "mq_unlink", SetLastError = true, StringMarshalling = StringMarshalling.Utf8)]
    public static partial int MqUnlink(string name);

    [LibraryImport(Libc, EntryPoint = "mq_getattr", SetLastError = true)]
    private static partial int MqGetattr(int queue, out MqAttr attributes);

    [LibraryImport(Libc, EntryPoint = "mq_timedsend", SetLastError = true)]
    private static unsafe partial int MqTimedsend(int queue, byte* message, nuint length, uint priority, ref TimeSpec deadline);

    [LibraryImport(Libc, EntryPoint = "mq_timedreceive", SetLastError = true)]
    private static unsafe partial nint MqTimedreceive(int queue, byte* message, nuint length, nint priority,
        ref TimeSpec deadline);

    [LibraryImport(Libc, EntryPoint = "msgget", SetLastError = true)]
    private static partial int Msgget(int key, int flags);

    [LibraryImport(Libc, EntryPoint = "msgsnd", SetLastError = true)]
    private static unsafe partial int Msgsnd(int queue, byte* message, nuint size, int flags);

    [LibraryImport(Libc, EntryPoint = "msgrcv", SetLastError = true)]
    private static unsafe partial nint Msgrcv(int queue, byte* message, nuint size, nint type, int flags);

    [LibraryImport(Libc, EntryPoint = "msgctl", SetLastError = true)]
    private static partial int Msgctl(int queue, int command, nint buffer);

    public static int LastErrno => Marshal.GetLastPInvokeError();

    public static bool IsAgain(int errno) => errno == (IsLinux ? EAGAIN_LINUX : EAGAIN_BSD);

    public static int ETimedOut => IsLinux ? 110 : 60;

    public static int ENoMsg => IsLinux ? 42 : OperatingSystem.IsMacOS() ? 91 : 83;

    public static int EIdRm => IsLinux ? 43 : OperatingSystem.IsMacOS() ? 90 : 82;

    public const int EINVAL = 22;

    private static int NonBlock => IsLinux ? 0x800 : 0x4;

    private static int CloseOnExec => IsLinux ? 0x80000 : OperatingSystem.IsMacOS() ? 0x1000000 : 0x100000;

    /// <summary>The error of the last call, mapped like the other POSIX failures.</summary>
    public static IpcException ErrorOf(string what) => Error(what);

    public static void Mkfifo(string path)
    {
        if (MkfifoNative(path, 0x180) == -1)
        {
            throw Error($"cannot create the FIFO `{path}`");
        }
    }

    /// <summary>Opens a FIFO end without blocking.</summary>
    public static int OpenFifo(string path, bool write)
    {
        var fd = OpenNative(path, (write ? 1 : 0) | NonBlock | CloseOnExec);
        if (fd == -1)
        {
            throw Error($"cannot open the FIFO `{path}`");
        }
        return fd;
    }

    /// <summary>Waits until <paramref name="fd"/> is ready for <paramref name="events"/>, 1 to 100 ms.</summary>
    public static bool Poll(int fd, short events, TimeSpan timeout)
    {
        var entry = new PollFd { Fd = fd, Events = events };
        var millis = (int)Math.Clamp(timeout.TotalMilliseconds, 1, 100);
        var result = PollNative(ref entry, 1, millis);
        if (result == -1)
        {
            return LastErrno == EINTR ? false : throw Error("cannot poll");
        }
        return result > 0;
    }

    public static unsafe nint Read(int fd, Span<byte> buffer)
    {
        fixed (byte* pointer = buffer)
        {
            return ReadNative(fd, pointer, (nuint)buffer.Length);
        }
    }

    public static unsafe nint Write(int fd, ReadOnlySpan<byte> data)
    {
        fixed (byte* pointer = data)
        {
            return WriteNative(fd, pointer, (nuint)data.Length);
        }
    }

    public static void CloseDescriptor(int fd) => _ = Close(fd);

    // ------------------------------------------------------------ POSIX message queues (Linux)

    private static TimeSpec DeadlineAfter(TimeSpan timeout)
    {
        _ = ClockGettime(0, out var now);
        var nanos = (long)now.Nanoseconds + (timeout.Ticks % TimeSpan.TicksPerSecond) * 100;
        return new TimeSpec
        {
            Seconds = now.Seconds + (nint)(long)timeout.TotalSeconds + (nint)(nanos / 1_000_000_000),
            Nanoseconds = (nint)(nanos % 1_000_000_000),
        };
    }

    public static int MqCreate(string name, long maxMessages, long messageSize)
    {
        var attributes = new MqAttr { MaxMessages = (nint)maxMessages, MessageSize = (nint)messageSize };
        var queue = MqOpenCreate(name, 2 | 0x40 | 0x80 | 0x80000, 0x180, ref attributes);
        return queue == -1 ? throw Error($"cannot create the POSIX message queue `{name}`") : queue;
    }

    public static int MqOpen(string name)
    {
        var queue = MqOpenExisting(name, 2 | 0x80000);
        return queue == -1 ? throw Error($"cannot open the POSIX message queue `{name}`") : queue;
    }

    public static int MqMessageSize(int queue) =>
        MqGetattr(queue, out var attributes) == -1 ? throw Error("cannot query the POSIX message queue")
            : (int)attributes.MessageSize;

    /// <summary>Sends within 10 ms; false when the queue stayed full.</summary>
    public static unsafe bool MqSend(int queue, ReadOnlySpan<byte> frame)
    {
        var deadline = DeadlineAfter(TimeSpan.FromMilliseconds(10));
        fixed (byte* pointer = frame)
        {
            if (MqTimedsend(queue, pointer, (nuint)frame.Length, 0, ref deadline) == 0)
            {
                return true;
            }
        }
        var errno = LastErrno;
        return errno == ETimedOut || IsAgain(errno) || errno == EINTR ? false : throw Error("cannot send a message");
    }

    public static unsafe byte[]? MqReceive(int queue, byte[] buffer, TimeSpan timeout)
    {
        var deadline = DeadlineAfter(timeout);
        nint read;
        fixed (byte* pointer = buffer)
        {
            read = MqTimedreceive(queue, pointer, (nuint)buffer.Length, 0, ref deadline);
        }
        if (read >= 0)
        {
            return buffer.AsSpan(0, (int)read).ToArray();
        }
        var errno = LastErrno;
        return errno == ETimedOut || IsAgain(errno) || errno == EINTR ? null : throw Error("cannot receive a message");
    }

    // ------------------------------------------------------------ System V message queues

    public static int SysvCreate()
    {
        var queue = Msgget(0, 0x200 | 0x180);
        return queue == -1 ? throw Error("cannot create a System V message queue") : queue;
    }

    public static void SysvRemove(int queue) => _ = Msgctl(queue, 0, 0);

    private static IpcException SysvGone(string what)
    {
        var errno = LastErrno;
        return errno == EIdRm || errno == EINVAL ? IpcException.HungUp() : Error(what);
    }

    /// <summary>Sends a <c>msgbuf</c> of type 1 without waiting; false when the queue is full.</summary>
    public static unsafe bool SysvSend(int queue, ReadOnlySpan<byte> frame)
    {
        var message = new byte[sizeof(nint) + frame.Length];
        BitConverter.TryWriteBytes(message, (long)1);
        if (sizeof(nint) == 4)
        {
            BitConverter.TryWriteBytes(message, 1);
        }
        frame.CopyTo(message.AsSpan(sizeof(nint)));
        fixed (byte* pointer = message)
        {
            if (Msgsnd(queue, pointer, (nuint)frame.Length, 0x800) == 0)
            {
                return true;
            }
        }
        var errno = LastErrno;
        return IsAgain(errno) || errno == EINTR ? false : throw SysvGone("cannot send a message");
    }

    /// <summary>Receives one message without waiting; null when the queue is empty.</summary>
    public static unsafe byte[]? SysvTryReceive(int queue, byte[] message)
    {
        nint read;
        fixed (byte* pointer = message)
        {
            read = Msgrcv(queue, pointer, (nuint)(message.Length - sizeof(nint)), 0, 0x800);
        }
        if (read >= 0)
        {
            return message.AsSpan(sizeof(nint), (int)read).ToArray();
        }
        var errno = LastErrno;
        return errno == ENoMsg || IsAgain(errno) || errno == EINTR ? null : throw SysvGone("cannot receive a message");
    }
}

/// <summary>Named pipes on POSIX (<c>pipe</c> and <c>fifo</c>): two FIFOs, one per direction.</summary>
[UnsupportedOSPlatform("windows")]
internal static class PosixFifo
{
    private sealed class FifoTransport : ITransport, IDisposable
    {
        private readonly int _incoming;
        private readonly Lock _outgoingLock = new();
        private readonly Lock _reading = new();
        private readonly PeerWatch _peer;
        private int _outgoing;
        private volatile bool _greeted;
        private volatile bool _closed;
        private bool _incomingClosed;

        public FifoTransport(string kind, int incoming, int outgoing, uint peer)
        {
            Kind = kind;
            _incoming = incoming;
            _outgoing = outgoing;
            _peer = new PeerWatch(peer);
            // The hello byte: tells the peer that this writer is open.
            var deadline = Deadline.After(TimeSpan.FromSeconds(10));
            while (Write([0]) == 0)
            {
                if (deadline.Passed)
                {
                    throw IpcException.TimedOut();
                }
            }
        }

        public string Kind { get; }

        public int Read(Span<byte> buffer, TimeSpan? timeout)
        {
            lock (_reading)
            {
                var deadline = Deadline.After(timeout);
                while (true)
                {
                    if (_closed)
                    {
                        return 0;
                    }
                    var wait = TimeSpan.FromMilliseconds(100);
                    if (deadline is { } limit)
                    {
                        if (limit.Passed)
                        {
                            throw IpcException.TimedOut();
                        }
                        wait = limit.Remaining;
                    }
                    if (!PosixNative.Poll(_incoming, PosixNative.PollIn, wait))
                    {
                        if (!_greeted && !_peer.Alive)
                        {
                            throw IpcException.HungUp();
                        }
                        continue;
                    }
                    var read = PosixNative.Read(_incoming, buffer);
                    if (read < 0)
                    {
                        var errno = PosixNative.LastErrno;
                        if (PosixNative.IsAgain(errno) || errno == PosixNative.EINTR)
                        {
                            continue;
                        }
                        throw PosixNative.ErrorOf("cannot read the FIFO");
                    }
                    if (read == 0)
                    {
                        if (_greeted)
                        {
                            return 0;
                        }
                        // The writer has not opened yet.
                        if (!_peer.Alive)
                        {
                            throw IpcException.HungUp();
                        }
                        Thread.Sleep(1);
                        continue;
                    }
                    if (!_greeted)
                    {
                        _greeted = true;
                        buffer[1..(int)read].CopyTo(buffer);
                        if (read == 1)
                        {
                            continue;
                        }
                        return (int)read - 1;
                    }
                    return (int)read;
                }
            }
        }

        public int Write(ReadOnlySpan<byte> data)
        {
            lock (_outgoingLock)
            {
                if (_outgoing < 0)
                {
                    throw new IpcException(IpcErrorKind.BrokenPipe, "the outgoing direction was closed");
                }
                while (true)
                {
                    if (_closed)
                    {
                        throw new IpcException(IpcErrorKind.BrokenPipe, "the connection was closed");
                    }
                    var written = PosixNative.Write(_outgoing, data);
                    if (written >= 0)
                    {
                        return (int)written;
                    }
                    var errno = PosixNative.LastErrno;
                    if (PosixNative.IsAgain(errno))
                    {
                        PosixNative.Poll(_outgoing, PosixNative.PollOut, TimeSpan.FromMilliseconds(100));
                    }
                    else if (errno != PosixNative.EINTR)
                    {
                        throw errno == 32 ? IpcException.BrokenPipe() : PosixNative.ErrorOf("cannot write the FIFO");
                    }
                }
            }
        }

        public void CloseWrite()
        {
            lock (_outgoingLock)
            {
                if (_outgoing >= 0)
                {
                    PosixNative.CloseDescriptor(_outgoing);
                    _outgoing = -1;
                }
            }
        }

        public void Shutdown()
        {
            if (_closed)
            {
                return;
            }
            _closed = true;
            IpcTransports.Trace($"{Kind} to process {_peer.Pid} hung up");
            CloseWrite();
        }

        public void Dispose()
        {
            Shutdown();
            lock (_reading)
            {
                if (!_incomingClosed)
                {
                    _incomingClosed = true;
                    PosixNative.CloseDescriptor(_incoming);
                }
            }
        }
    }

    private static string FifoKind(Endpoint endpoint) => endpoint.Kind == "pipe" ? "pipe" : "fifo";

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IpcStream Connect(Endpoint endpoint)
    {
        var kind = FifoKind(endpoint);
        var rendezvous = new Rendezvous(endpoint, "fifo");
        var (c2s, s2c) = (rendezvous.PathOf("c2s"), rendezvous.PathOf("s2c"));
        var incoming = -1;
        try
        {
            PosixNative.Mkfifo(c2s);
            PosixNative.Mkfifo(s2c);
            incoming = PosixNative.OpenFifo(s2c, false);
            var ack = rendezvous.Request(new IpcJson());
            var peer = (uint)(ack.Number("pid") ?? rendezvous.ServerPid);
            var outgoing = PosixNative.OpenFifo(c2s, true);
            FifoTransport transport;
            try
            {
                transport = new FifoTransport(kind, incoming, outgoing, peer);
            }
            catch
            {
                PosixNative.CloseDescriptor(outgoing);
                throw;
            }
            incoming = -1;
            IpcTransports.Trace($"{kind} connected as {rendezvous.Id}");
            return new IpcStream(transport);
        }
        finally
        {
            if (incoming >= 0)
            {
                PosixNative.CloseDescriptor(incoming);
            }
            RendezvousFiles.TryDelete(c2s);
            RendezvousFiles.TryDelete(s2c);
        }
    }

    private sealed class FifoListener(Endpoint endpoint) : IIpcListener
    {
        private readonly RendezvousListener _rendezvous = new(endpoint, "fifo");
        private readonly string _kind = FifoKind(endpoint);

        public IpcStream? Accept() => _rendezvous.AcceptWith(_kind, request =>
        {
            var outgoing = PosixNative.OpenFifo(request.PathOf("s2c"), true);
            int incoming;
            try
            {
                incoming = PosixNative.OpenFifo(request.PathOf("c2s"), false);
            }
            catch
            {
                PosixNative.CloseDescriptor(outgoing);
                throw;
            }
            try
            {
                return (new IpcJson(), new IpcStream(new FifoTransport(_kind, incoming, outgoing, request.Pid)));
            }
            catch
            {
                PosixNative.CloseDescriptor(outgoing);
                PosixNative.CloseDescriptor(incoming);
                throw;
            }
        });

        public string Endpoint => _rendezvous.Endpoint;

        public Action Closer() => _rendezvous.Closer();

        public void Dispose() => _rendezvous.Dispose();
    }

    public static IIpcListener Listen(Endpoint endpoint) => new FifoListener(endpoint);
}

/// <summary>POSIX message queues (<c>mq</c>, Linux): one queue per direction, named <c>/lcIDs</c> and <c>/lcIDc</c>.</summary>
[SupportedOSPlatform("linux")]
internal static class PosixMessageQueue
{
    private static string QueueName(string id, char suffix) => $"/lc{id}{suffix}";

    private sealed class MqLink(int send, int receive, uint peer) : IDatagrams
    {
        private readonly byte[] _buffer = new byte[PosixNative.MqMessageSize(receive)];
        private readonly PeerWatch _peer = new(peer);

        public bool Send(ReadOnlySpan<byte> frame) => PosixNative.MqSend(send, frame);

        public byte[]? Receive(TimeSpan timeout) => PosixNative.MqReceive(receive, _buffer, timeout);

        public bool PeerAlive => _peer.Alive;

        public void Close()
        {
            _ = PosixNative.MqClose(send);
            _ = PosixNative.MqClose(receive);
        }
    }

    private static IDatagrams Link(int send, int receive, uint peer)
    {
        try
        {
            return new MqLink(send, receive, peer);
        }
        catch
        {
            _ = PosixNative.MqClose(send);
            _ = PosixNative.MqClose(receive);
            throw;
        }
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IpcStream Connect(Endpoint endpoint)
    {
        var rendezvous = new Rendezvous(endpoint, "mq");
        var ownChunk = endpoint.Number("chunk", 4096, 16, 65000);
        var maxMessages = endpoint.Number("maxmsg", 8, 1, 65536);
        var (serverName, clientName) = (QueueName(rendezvous.Id, 's'), QueueName(rendezvous.Id, 'c'));
        var size = (long)ownChunk + DatagramSession.HeaderSize;
        int send = -1, receive = -1;
        try
        {
            try
            {
                send = PosixNative.MqCreate(serverName, (long)maxMessages, size);
                receive = PosixNative.MqCreate(clientName, (long)maxMessages, size);
            }
            catch (IpcException error)
            {
                throw new IpcException(error.Kind, $"cannot create the POSIX message queues: {error.Message}", error);
            }
            IpcJson ack;
            try
            {
                ack = rendezvous.Request(new IpcJson().With("chunk", ownChunk));
            }
            finally
            {
                _ = PosixNative.MqUnlink(serverName);
                _ = PosixNative.MqUnlink(clientName);
            }
            var chunk = DatagramSession.AgreeChunk(ownChunk, ack.Number("chunk"));
            var peer = (uint)(ack.Number("pid") ?? rendezvous.ServerPid);
            var link = Link(send, receive, peer);
            (send, receive) = (-1, -1);
            var session = new DatagramSession("mq", link, DatagramSettings.Local(endpoint, chunk));
            IpcTransports.Trace($"mq connected as {rendezvous.Id}, chunk {chunk}");
            return new IpcStream(session);
        }
        catch
        {
            _ = PosixNative.MqUnlink(serverName);
            _ = PosixNative.MqUnlink(clientName);
            if (send >= 0)
            {
                _ = PosixNative.MqClose(send);
            }
            if (receive >= 0)
            {
                _ = PosixNative.MqClose(receive);
            }
            throw;
        }
    }

    private sealed class MqListener(Endpoint endpoint) : IIpcListener
    {
        private readonly RendezvousListener _rendezvous = new(endpoint, "mq");

        public IpcStream? Accept() => _rendezvous.AcceptWith("mq", request =>
        {
            var ownChunk = endpoint.Number("chunk", 4096, 16, 65000);
            var chunk = DatagramSession.AgreeChunk(ownChunk, request.Fields.Number("chunk"));
            var receive = PosixNative.MqOpen(QueueName(request.Id, 's'));
            int send;
            try
            {
                send = PosixNative.MqOpen(QueueName(request.Id, 'c'));
            }
            catch
            {
                _ = PosixNative.MqClose(receive);
                throw;
            }
            var session = new DatagramSession("mq", Link(send, receive, request.Pid), DatagramSettings.Local(endpoint, chunk));
            return (new IpcJson().With("chunk", (ulong)chunk), new IpcStream(session));
        });

        public string Endpoint => _rendezvous.Endpoint;

        public Action Closer() => _rendezvous.Closer();

        public void Dispose() => _rendezvous.Dispose();
    }

    public static IIpcListener Listen(Endpoint endpoint) => new MqListener(endpoint);
}

/// <summary>System V message queues (<c>sysvmq</c>, Linux and macOS): one private queue per direction.</summary>
[UnsupportedOSPlatform("windows")]
internal static class SysVMessageQueue
{
    private const int MaxFrame = 65000 + DatagramSession.HeaderSize;

    private sealed class SysvLink(int send, int receive, TimeSpan poll, uint peer) : IDatagrams
    {
        private readonly byte[] _message = new byte[IntPtr.Size + MaxFrame];
        private readonly PeerWatch _peer = new(peer);
        private int _removed;

        public bool Send(ReadOnlySpan<byte> frame) => PosixNative.SysvSend(send, frame);

        public byte[]? Receive(TimeSpan timeout)
        {
            var started = Stopwatch.GetTimestamp();
            var backoff = new Backoff(poll);
            while (true)
            {
                if (PosixNative.SysvTryReceive(receive, _message) is { } frame)
                {
                    return frame;
                }
                if (Stopwatch.GetElapsedTime(started) >= timeout)
                {
                    return null;
                }
                backoff.Sleep();
            }
        }

        public bool PeerAlive => _peer.Alive;

        public void Close()
        {
            if (Interlocked.Exchange(ref _removed, 1) == 0)
            {
                PosixNative.SysvRemove(send);
                PosixNative.SysvRemove(receive);
            }
        }
    }

    private static IDatagrams Link(Endpoint endpoint, int send, int receive, uint peer)
    {
        var poll = endpoint.Millis("poll", 5);
        var floor = TimeSpan.FromTicks(500);
        return new SysvLink(send, receive, poll < floor ? floor : poll, peer);
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IpcStream Connect(Endpoint endpoint)
    {
        var rendezvous = new Rendezvous(endpoint, "sysvmq");
        var ownChunk = endpoint.Number("chunk", 1024, 16, 65000);
        var serverQueue = PosixNative.SysvCreate();
        int clientQueue;
        try
        {
            clientQueue = PosixNative.SysvCreate();
        }
        catch
        {
            PosixNative.SysvRemove(serverQueue);
            throw;
        }
        IpcJson ack;
        try
        {
            ack = rendezvous.Request(new IpcJson()
                .With("chunk", ownChunk)
                .With("server_queue", (ulong)serverQueue)
                .With("client_queue", (ulong)clientQueue));
        }
        catch
        {
            PosixNative.SysvRemove(serverQueue);
            PosixNative.SysvRemove(clientQueue);
            throw;
        }
        var chunk = DatagramSession.AgreeChunk(ownChunk, ack.Number("chunk"));
        var peer = (uint)(ack.Number("pid") ?? rendezvous.ServerPid);
        var session = new DatagramSession("sysvmq", Link(endpoint, serverQueue, clientQueue, peer),
            DatagramSettings.Local(endpoint, chunk));
        IpcTransports.Trace($"sysvmq connected as {rendezvous.Id}, chunk {chunk}");
        return new IpcStream(session);
    }

    private sealed class SysvListener(Endpoint endpoint) : IIpcListener
    {
        private readonly RendezvousListener _rendezvous = new(endpoint, "sysvmq");

        public IpcStream? Accept() => _rendezvous.AcceptWith("sysvmq", request =>
        {
            var ownChunk = endpoint.Number("chunk", 1024, 16, 65000);
            var chunk = DatagramSession.AgreeChunk(ownChunk, request.Fields.Number("chunk"));
            int Queue(string name)
            {
                var id = request.Fields.RequireNumber(name);
                return id <= int.MaxValue ? (int)id : throw IpcException.InvalidData("an invalid queue id");
            }
            var (receive, send) = (Queue("server_queue"), Queue("client_queue"));
            var session = new DatagramSession("sysvmq", Link(endpoint, send, receive, request.Pid),
                DatagramSettings.Local(endpoint, chunk));
            return (new IpcJson().With("chunk", (ulong)chunk), new IpcStream(session));
        });

        public string Endpoint => _rendezvous.Endpoint;

        public Action Closer() => _rendezvous.Closer();

        public void Dispose() => _rendezvous.Dispose();
    }

    public static IIpcListener Listen(Endpoint endpoint) => new SysvListener(endpoint);
}
