using System.Diagnostics.CodeAnalysis;
using System.Diagnostics;
using System.IO.MemoryMappedFiles;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>
/// One side of the two byte rings shared by <c>shm</c>, <c>mmap</c>, <c>file</c> and <c>signal</c> (see "Ring
/// region" in <c>docs/protocol/ipc-transports-v1.md</c>).
/// </summary>
internal sealed class RingTransport : ITransport, IDisposable
{
    public static readonly byte[] Magic = "LIPC"u8.ToArray();
    public const uint Version = 1;
    public const long Header = 64;
    public const long RingHeader = 128;
    private const long Head = 0;
    private const long WriterState = 8;
    private const long Tail = 64;
    private const long ReaderState = 72;
    private const uint Open = 0;
    private const uint Ended = 1;
    private const uint Aborted = 2;
    private const uint Gone = 2;
    private static readonly TimeSpan PeerCheck = TimeSpan.FromMilliseconds(100);

    private readonly IRingRegion _region;
    private readonly uint _capacity;
    private readonly long _outgoing;
    private readonly long _incoming;
    private readonly IRingNotifier _notifier;
    private readonly PeerWatch _peer;
    private readonly string[] _cleanup;
    private readonly Lock _reading = new();
    private readonly Lock _writing = new();
    private int _closed;
    private int _disposed;

    public RingTransport(string kind, IRingRegion region, bool client, IRingNotifier notifier, uint peerPid,
        string[] cleanup)
    {
        _capacity = Validate(region);
        (_outgoing, _incoming) = client
            ? (RingBase(0, _capacity), RingBase(1, _capacity))
            : (RingBase(1, _capacity), RingBase(0, _capacity));
        _region = region;
        _notifier = notifier;
        _peer = new PeerWatch(peerPid);
        _cleanup = cleanup;
        Kind = kind;
    }

    public string Kind { get; }

    public static long RegionSize(uint capacity) => Header + 2 * (RingHeader + capacity);

    private static long RingBase(int ring, uint capacity) => Header + ring * (RingHeader + capacity);

    private uint Counter(long offset)
    {
        for (var attempt = 0; ; attempt++)
        {
            var value = _region.Load(offset);
            var complement = _region.Load(offset + 4);
            if (value == ~complement)
            {
                return value;
            }
            if (attempt >= 1_000_000)
            {
                throw IpcException.InvalidData("a ring counter stays torn");
            }
            if (attempt >= 64)
            {
                Thread.Yield();
            }
            else
            {
                Thread.SpinWait(1);
            }
        }
    }

    private void Publish(long offset, uint value)
    {
        _region.Store(offset, value);
        _region.Store(offset + 4, ~value);
    }

    /// <summary>(head, tail) of a ring, consistent with each other.</summary>
    private (uint Head, uint Tail) Positions(long ringBase)
    {
        while (true)
        {
            var head = Counter(ringBase + Head);
            var tail = Counter(ringBase + Tail);
            if (unchecked(head - tail) <= _capacity)
            {
                return (head, tail);
            }
            Thread.Yield();
        }
    }

    private void CopyOut(long ringBase, uint position, Span<byte> buffer)
    {
        var data = ringBase + RingHeader;
        var start = position % _capacity;
        var first = (int)Math.Min(buffer.Length, _capacity - start);
        _region.ReadBytes(data + start, buffer[..first]);
        if (first < buffer.Length)
        {
            _region.ReadBytes(data, buffer[first..]);
        }
    }

    private void CopyIn(long ringBase, uint position, ReadOnlySpan<byte> bytes)
    {
        var data = ringBase + RingHeader;
        var start = position % _capacity;
        var first = (int)Math.Min(bytes.Length, _capacity - start);
        _region.WriteBytes(data + start, bytes[..first]);
        if (first < bytes.Length)
        {
            _region.WriteBytes(data, bytes[first..]);
        }
    }

    private bool Closed => Volatile.Read(ref _closed) != 0;

    public int Read(Span<byte> buffer, TimeSpan? timeout)
    {
        lock (_reading)
        {
            var deadline = Deadline.After(timeout);
            uint attempt = 0;
            var lastCheck = Stopwatch.GetTimestamp();
            while (true)
            {
                var generation = _notifier.Generation;
                var (head, tail) = Positions(_incoming);
                var used = unchecked(head - tail);
                if (used > 0)
                {
                    var count = (int)Math.Min(used, (uint)buffer.Length);
                    CopyOut(_incoming, tail, buffer[..count]);
                    Publish(_incoming + Tail, unchecked(tail + (uint)count));
                    _notifier.Notify();
                    return count;
                }
                if (Closed)
                {
                    return 0;
                }
                switch (_region.Load(_incoming + WriterState))
                {
                    case Open:
                        break;
                    case Aborted:
                        throw IpcException.HungUp();
                    default:
                        // The writer may have written its last bytes after the positions above were read and
                        // before it ended.
                        var (lastHead, lastTail) = Positions(_incoming);
                        if (lastHead != lastTail)
                        {
                            continue;
                        }
                        return 0;
                }
                if (Stopwatch.GetElapsedTime(lastCheck) >= PeerCheck)
                {
                    lastCheck = Stopwatch.GetTimestamp();
                    if (!_peer.Alive)
                    {
                        throw IpcException.HungUp();
                    }
                }
                var limit = PeerCheck;
                if (deadline is { } end)
                {
                    if (end.Passed)
                    {
                        throw IpcException.TimedOut();
                    }
                    limit = end.Remaining < PeerCheck ? end.Remaining : PeerCheck;
                }
                _notifier.Wait(generation, attempt, limit);
                attempt = attempt == uint.MaxValue ? attempt : attempt + 1;
            }
        }
    }

    public int Write(ReadOnlySpan<byte> data)
    {
        lock (_writing)
        {
            uint attempt = 0;
            var lastCheck = Stopwatch.GetTimestamp();
            while (true)
            {
                if (Closed || _region.Load(_outgoing + WriterState) != Open)
                {
                    throw new IpcException(IpcErrorKind.BrokenPipe, "the outgoing direction was closed");
                }
                if (_region.Load(_outgoing + ReaderState) == Gone)
                {
                    throw IpcException.BrokenPipe();
                }
                var generation = _notifier.Generation;
                var (head, tail) = Positions(_outgoing);
                var free = _capacity - unchecked(head - tail);
                if (free > 0)
                {
                    var count = (int)Math.Min(free, (uint)data.Length);
                    CopyIn(_outgoing, head, data[..count]);
                    Publish(_outgoing + Head, unchecked(head + (uint)count));
                    _notifier.Notify();
                    return count;
                }
                if (Stopwatch.GetElapsedTime(lastCheck) >= PeerCheck)
                {
                    lastCheck = Stopwatch.GetTimestamp();
                    if (!_peer.Alive)
                    {
                        throw IpcException.BrokenPipe();
                    }
                }
                _notifier.Wait(generation, attempt, PeerCheck);
                attempt = attempt == uint.MaxValue ? attempt : attempt + 1;
            }
        }
    }

    public void CloseWrite()
    {
        lock (_writing)
        {
            if (_region.Load(_outgoing + WriterState) == Open)
            {
                _region.Store(_outgoing + WriterState, Ended);
                _notifier.Notify();
            }
        }
    }

    public void Shutdown()
    {
        if (Interlocked.Exchange(ref _closed, 1) != 0)
        {
            return;
        }
        IpcTransports.Trace($"{Kind} ring to process {_peer.Pid} hung up");
        try
        {
            if (_region.Load(_outgoing + WriterState) == Open)
            {
                _region.Store(_outgoing + WriterState, Ended);
            }
            _region.Store(_incoming + ReaderState, Gone);
            _notifier.Notify();
        }
        catch (Exception error) when (error is IOException or ObjectDisposedException)
        {
        }
    }

    public void Dispose()
    {
        if (Interlocked.Exchange(ref _disposed, 1) != 0)
        {
            return;
        }
        Shutdown();
        // Wait for a read or write in progress before unmapping the region under it.
        lock (_reading)
        {
            lock (_writing)
            {
                _notifier.Dispose();
                _region.Dispose();
            }
        }
        // After the region: Windows cannot delete mapped files.
        foreach (var path in _cleanup)
        {
            RendezvousFiles.TryDelete(path);
        }
    }

    /// <summary>Checks the header and returns the capacity.</summary>
    private static uint Validate(IRingRegion region)
    {
        if (region.Length < Header)
        {
            throw IpcException.InvalidData("the ring region is too small");
        }
        Span<byte> magic = stackalloc byte[4];
        region.ReadBytes(0, magic);
        if (!magic.SequenceEqual(Magic))
        {
            throw IpcException.InvalidData("the ring region lacks the `LIPC` magic");
        }
        if (region.Load(4) != Version)
        {
            throw IpcException.InvalidData("the ring region has another version");
        }
        var capacity = region.Load(8);
        if (capacity is < 64 or > 1 << 30 || RegionSize(capacity) > region.Length)
        {
            throw IpcException.InvalidData("the ring region has an invalid capacity");
        }
        return capacity;
    }

    /// <summary>Writes the header of a fresh, zero-filled region.</summary>
    public static void Initialize(IRingRegion region, uint capacity)
    {
        region.WriteBytes(0, Magic);
        region.Store(4, Version);
        region.Store(8, capacity);
        for (var ring = 0; ring < 2; ring++)
        {
            var ringBase = RingBase(ring, capacity);
            region.Store(ringBase + Head + 4, uint.MaxValue);
            region.Store(ringBase + Tail + 4, uint.MaxValue);
        }
    }
}

/// <summary>Connects and listens on the ring kinds.</summary>
internal static class RingTransports
{
    private static string RingKind(Endpoint endpoint) => endpoint.Kind switch
    {
        "shm" => "shm",
        "mmap" => "mmap",
        "file" => "file",
        _ => "signal",
    };

    private static string FileSuffix(string kind) => kind switch
    {
        "mmap" => "mmap",
        "file" => "ring",
        _ => "sig",
    };

    private static FileStream CreateFile(string path, long size)
    {
        var options = new FileStreamOptions
        {
            Mode = FileMode.CreateNew,
            Access = FileAccess.ReadWrite,
            Share = FileShare.ReadWrite | FileShare.Delete,
        };
        if (!OperatingSystem.IsWindows())
        {
            options.UnixCreateMode = UnixFileMode.UserRead | UnixFileMode.UserWrite;
        }
        var file = new FileStream(path, options);
        try
        {
            file.SetLength(size);
        }
        catch
        {
            file.Dispose();
            throw;
        }
        return file;
    }

    private static FileStream OpenFile(string path) =>
        new(path, FileMode.Open, FileAccess.ReadWrite, FileShare.ReadWrite | FileShare.Delete);

    private static IRingRegion FileRegionOf(string kind, FileStream file, long length)
    {
        if (kind == "file")
        {
            return new FileRegion(file.SafeFileHandle, length);
        }
        return MappedRegion.OfFile(file, length);
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    private static IRingRegion CreateShm(string id, long size)
    {
        if (OperatingSystem.IsWindows())
        {
            var name = $"Local\\clink-{id}";
            MemoryMappedFile mapped;
            try
            {
                mapped = MemoryMappedFile.CreateNew(name, size, MemoryMappedFileAccess.ReadWrite);
            }
            catch (IOException error)
            {
                throw new IpcException(IpcErrorKind.AlreadyExists, $"the shared memory `{name}` already exists", error);
            }
            return MappedRegion.OfMapping(mapped, size);
        }
        return PosixNative.CreateShm(id, size);
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    private static IRingRegion OpenShm(string id)
    {
        if (OperatingSystem.IsWindows())
        {
            return MappedRegion.OfMapping(
                MemoryMappedFile.OpenExisting($"Local\\clink-{id}", MemoryMappedFileRights.ReadWrite), 0);
        }
        return PosixNative.OpenShm(id);
    }

    private static void UnlinkShm(string id)
    {
        if (!OperatingSystem.IsWindows())
        {
            PosixNative.UnlinkShm(id);
        }
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IpcStream Connect(Endpoint endpoint)
    {
        var kind = RingKind(endpoint);
        if (kind == "signal" && !IpcTransports.Supports("signal", false))
        {
            throw IpcException.Unsupported("signal", "connecting");
        }
        var rendezvous = new Rendezvous(endpoint, kind);
        var capacity = (uint)endpoint.Number("capacity", 65536, 64, 1UL << 30);
        var size = RingTransport.RegionSize(capacity);
        var files = new List<string>();
        IRingRegion region;
        if (kind == "shm")
        {
            region = CreateShm(rendezvous.Id, size);
        }
        else
        {
            var path = rendezvous.PathOf(FileSuffix(kind));
            var file = CreateFile(path, size);
            files.Add(path);
            try
            {
                region = FileRegionOf(kind, file, size);
            }
            catch
            {
                file.Dispose();
                RendezvousFiles.TryDelete(path);
                throw;
            }
        }
        void Forget()
        {
            if (kind == "shm")
            {
                UnlinkShm(rendezvous.Id);
            }
            foreach (var path in files)
            {
                RendezvousFiles.TryDelete(path);
            }
        }
        uint peer;
        IRingNotifier notifier;
        try
        {
            RingTransport.Initialize(region, capacity);
            var poll = endpoint.Millis("poll", 5);
            IRingSignalSetup? signal = null;
            var fields = new IpcJson();
            if (kind == "signal")
            {
                (fields, signal) = RingSignals.Client(endpoint, rendezvous.Id);
            }
            try
            {
                var ack = rendezvous.Request(fields);
                peer = (uint)(ack.Number("pid") ?? rendezvous.ServerPid);
                notifier = signal is not null ? signal.Finish(ack, peer) : new PollNotifier(poll);
            }
            catch
            {
                signal?.Dispose();
                throw;
            }
        }
        catch
        {
            region.Dispose();
            Forget();
            throw;
        }
        // Both sides hold the region open now; nothing should outlive a crash.
        string[] cleanup;
        if (OperatingSystem.IsWindows())
        {
            cleanup = [.. files];
        }
        else
        {
            Forget();
            cleanup = [];
        }
        var transport = new RingTransport(kind, region, true, notifier, peer, cleanup);
        IpcTransports.Trace($"{kind} connected as {rendezvous.Id} to process {peer}, {capacity} bytes per ring");
        return new IpcStream(transport);
    }

    public static IIpcListener Listen(Endpoint endpoint)
    {
        var kind = RingKind(endpoint);
        if (kind is "shm" or "signal" && !IpcTransports.Supports(kind, true))
        {
            throw IpcException.Unsupported(kind, "listening");
        }
        return new RingListener(endpoint, kind);
    }

    private sealed class RingListener(Endpoint endpoint, string kind) : IIpcListener
    {
        private readonly RendezvousListener _rendezvous = new(endpoint, kind);

        [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
        private (IRingRegion Region, string[] Cleanup) OpenRegion(RendezvousRequest request)
        {
            if (kind == "shm")
            {
                return (OpenShm(request.Id), []);
            }
            var path = request.PathOf(FileSuffix(kind));
            var file = OpenFile(path);
            try
            {
                return (FileRegionOf(kind, file, file.Length), OperatingSystem.IsWindows() ? [path] : []);
            }
            catch
            {
                file.Dispose();
                throw;
            }
        }

        public IpcStream? Accept() => _rendezvous.AcceptWith(kind, request =>
        {
            var peer = request.Pid;
            var (region, cleanup) = OpenRegion(request);
            IRingNotifier? notifier = null;
            try
            {
                var fields = new IpcJson();
                if (kind == "signal")
                {
                    (fields, notifier) = RingSignals.Server(endpoint, request.Id, request.Fields, peer);
                }
                else
                {
                    notifier = new PollNotifier(endpoint.Millis("poll", 5));
                }
                return (fields, new IpcStream(new RingTransport(kind, region, false, notifier, peer, cleanup)));
            }
            catch
            {
                notifier?.Dispose();
                region.Dispose();
                throw;
            }
        });

        public string Endpoint => _rendezvous.Endpoint;

        public Action Closer() => _rendezvous.Closer();

        public void Dispose() => _rendezvous.Dispose();
    }
}

/// <summary>The client's signal setup before the request, finished with the listener's answer.</summary>
internal interface IRingSignalSetup : IDisposable
{
    IRingNotifier Finish(IpcJson ack, uint peer);
}

/// <summary>The notifiers of the <c>signal</c> kind: POSIX signals, or named events on Windows.</summary>
internal static class RingSignals
{
    public static (IpcJson Fields, IRingSignalSetup Setup) Client(Endpoint endpoint, string id)
    {
        if (OperatingSystem.IsWindows())
        {
            return WindowsEvents.Client(id);
        }
        return PosixSignals.Client(endpoint);
    }

    public static (IpcJson Fields, IRingNotifier Notifier) Server(Endpoint endpoint, string id, IpcJson request, uint peer)
    {
        if (OperatingSystem.IsWindows())
        {
            return WindowsEvents.Server(id);
        }
        return PosixSignals.Server(endpoint, request, peer);
    }
}
