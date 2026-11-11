using System.Diagnostics.CodeAnalysis;
using System.Collections.Concurrent;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>The few libc calls the POSIX mechanisms need beyond what .NET offers.</summary>
[UnsupportedOSPlatform("windows")]
internal static partial class PosixNative
{
    private const string Libc = "libc";
    public const int EPERM = 1;
    public const int EINTR = 4;
    public const int EAGAIN_LINUX = 11;
    public const int EAGAIN_BSD = 35;

    [DllImport(Libc, EntryPoint = "kill", SetLastError = true, ExactSpelling = true)]
    private static extern int Kill(int pid, int signal);

    [DllImport(Libc, EntryPoint = "dup2", SetLastError = true, ExactSpelling = true)]
    private static extern int Dup2(int from, int to);

    [DllImport(Libc, EntryPoint = "mmap", SetLastError = true, ExactSpelling = true)]
    private static extern nint Mmap(nint address, nuint length, int protection, int flags, int fd, long offset);

    [DllImport(Libc, EntryPoint = "munmap", SetLastError = true, ExactSpelling = true)]
    private static extern int Munmap(nint address, nuint length);

    [DllImport(Libc, EntryPoint = "shm_open", SetLastError = true, ExactSpelling = true)]
    private static extern int ShmOpen([MarshalAs(UnmanagedType.LPUTF8Str)] string name, int flags, uint mode);

    /// <summary>
    /// <c>shm_open</c> is variadic: on Apple arm64 a variadic argument goes on the stack, so six dummy register
    /// arguments push the mode past the eight argument registers.
    /// </summary>
    [DllImport(Libc, EntryPoint = "shm_open", SetLastError = true, ExactSpelling = true)]
    private static extern int ShmOpenVariadic([MarshalAs(UnmanagedType.LPUTF8Str)] string name, int flags, int a, int b, int c, int d, int e, int f, long mode);

    [DllImport(Libc, EntryPoint = "shm_unlink", SetLastError = true, ExactSpelling = true)]
    private static extern int ShmUnlink([MarshalAs(UnmanagedType.LPUTF8Str)] string name);

    [DllImport(Libc, EntryPoint = "ftruncate", SetLastError = true, ExactSpelling = true)]
    private static extern int Ftruncate(int fd, long length);

    [DllImport(Libc, EntryPoint = "fstat", SetLastError = true, ExactSpelling = true)]
    private static unsafe extern int Fstat(int fd, byte* stat);

    [DllImport(Libc, EntryPoint = "fstat$INODE64", SetLastError = true, ExactSpelling = true)]
    private static unsafe extern int FstatInode64(int fd, byte* stat);

    [DllImport(Libc, EntryPoint = "close", SetLastError = true, ExactSpelling = true)]
    private static extern int Close(int fd);

    private static int Errno => Marshal.GetLastPInvokeError();

    private static IpcException Error(string what)
    {
        var code = Errno;
        var kind = code switch
        {
            EPERM or 13 => IpcErrorKind.PermissionDenied,
            2 => IpcErrorKind.NotFound,
            17 => IpcErrorKind.AlreadyExists,
            _ => IpcErrorKind.Other,
        };
        return new IpcException(kind, $"{what}: {Marshal.GetPInvokeErrorMessage(code)} (os error {code})");
    }

    /// <summary>Whether the process <paramref name="pid"/> is still running.</summary>
    public static bool ProcessAlive(uint pid)
    {
        if (pid == 0 || pid > int.MaxValue)
        {
            return false;
        }
        return Kill((int)pid, 0) == 0 || Errno == EPERM;
    }

    /// <summary>Sends <paramref name="signal"/> to <paramref name="pid"/>, ignoring failures.</summary>
    public static void Signal(uint pid, int signal)
    {
        if (pid is > 0 and <= int.MaxValue)
        {
            _ = Kill((int)pid, signal);
        }
    }

    /// <summary>Replaces standard output with <c>/dev/null</c>, so the reader of the original one sees its end.</summary>
    public static void CloseStdout()
    {
        using var nullDevice = File.OpenHandle("/dev/null", FileMode.Open, FileAccess.Write);
        if (Dup2((int)nullDevice.DangerousGetHandle(), 1) == -1)
        {
            throw Error("cannot replace standard output");
        }
    }

    // ------------------------------------------------------------ shared memory

    private static bool IsLinux => OperatingSystem.IsLinux();

    private static (int Create, int Exclusive) CreateFlags => IsLinux ? (0x40, 0x80) : (0x200, 0x800);

    private static string ShmName(string id) => $"/lc{id}";

    /// <summary>On Linux <c>shm_open</c> is a file in <c>/dev/shm</c>; .NET maps it like any file.</summary>
    private static string LinuxShmPath(string id) => $"/dev/shm/lc{id}";

    private static int OpenShmDescriptor(string name, int flags)
    {
        var fd = OperatingSystem.IsMacOS() && RuntimeInformation.ProcessArchitecture == Architecture.Arm64
            ? ShmOpenVariadic(name, flags, 0, 0, 0, 0, 0, 0, 0x180)
            : ShmOpen(name, flags, 0x180);
        if (fd == -1)
        {
            throw Error($"cannot open the shared memory `{name}`");
        }
        return fd;
    }

    private static unsafe MappedRegion Map(int fd, long length)
    {
        if (length == 0)
        {
            throw IpcException.InvalidData("cannot map an empty region");
        }
        var address = Mmap(0, (nuint)length, 3, 1, fd, 0);
        if (address == -1)
        {
            throw Error("cannot map the shared memory");
        }
        return new MappedRegion((byte*)address, length, () => Munmap(address, (nuint)length));
    }

    private static unsafe long DescriptorSize(int fd)
    {
        var buffer = stackalloc byte[512];
        var result = OperatingSystem.IsMacOS() && RuntimeInformation.ProcessArchitecture == Architecture.X64
            ? FstatInode64(fd, buffer)
            : Fstat(fd, buffer);
        if (result == -1)
        {
            throw Error("cannot query the shared memory");
        }
        // The offset of `st_size`: 96 in the 64-bit inode `stat` of Apple systems, 112 on FreeBSD.
        return *(long*)(buffer + (OperatingSystem.IsMacOS() ? 96 : 112));
    }

    /// <summary>Creates the POSIX shared memory object <c>/lcID</c>.</summary>
    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IRingRegion CreateShm(string id, long length)
    {
        if (IsLinux)
        {
            var path = LinuxShmPath(id);
            FileStream file;
            try
            {
                file = new FileStream(path, new FileStreamOptions
                {
                    Mode = FileMode.CreateNew,
                    Access = FileAccess.ReadWrite,
                    Share = FileShare.ReadWrite | FileShare.Delete,
                    UnixCreateMode = UnixFileMode.UserRead | UnixFileMode.UserWrite,
                });
            }
            catch (IOException error) when (File.Exists(path))
            {
                throw new IpcException(IpcErrorKind.AlreadyExists, $"the shared memory `{ShmName(id)}` already exists", error);
            }
            try
            {
                file.SetLength(length);
                return MappedRegion.OfFile(file, length);
            }
            catch
            {
                file.Dispose();
                UnlinkShm(id);
                throw;
            }
        }
        var (create, exclusive) = CreateFlags;
        var fd = OpenShmDescriptor(ShmName(id), 2 | create | exclusive);
        try
        {
            if (Ftruncate(fd, length) == -1)
            {
                throw Error("cannot size the shared memory");
            }
            return Map(fd, length);
        }
        catch
        {
            UnlinkShm(id);
            throw;
        }
        finally
        {
            _ = Close(fd);
        }
    }

    /// <summary>Opens the POSIX shared memory object <c>/lcID</c>.</summary>
    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IRingRegion OpenShm(string id)
    {
        if (IsLinux)
        {
            var file = new FileStream(LinuxShmPath(id), FileMode.Open, FileAccess.ReadWrite,
                FileShare.ReadWrite | FileShare.Delete);
            try
            {
                return MappedRegion.OfFile(file, file.Length);
            }
            catch
            {
                file.Dispose();
                throw;
            }
        }
        var fd = OpenShmDescriptor(ShmName(id), 2);
        try
        {
            return Map(fd, DescriptorSize(fd));
        }
        finally
        {
            _ = Close(fd);
        }
    }

    /// <summary>Removes the name of <c>/lcID</c>; mappings stay valid.</summary>
    public static void UnlinkShm(string id)
    {
        if (IsLinux)
        {
            RendezvousFiles.TryDelete(LinuxShmPath(id));
        }
        else
        {
            _ = ShmUnlink(ShmName(id));
        }
    }
}

/// <summary>The POSIX <c>signal</c> notifier: each side catches its own signal and sends the peer's.</summary>
[UnsupportedOSPlatform("windows")]
internal static class PosixSignals
{
    private static readonly GenerationWake Wake = new();
    private static readonly ConcurrentDictionary<int, PosixSignalRegistration> Installed = new();
    private static readonly Lock InstallLock = new();

    private static int SigStop => OperatingSystem.IsLinux() ? 19 : 17;

    /// <summary>Catches <paramref name="signo"/> from now on, turning it into ring wake-ups.</summary>
    private static void Install(int signo)
    {
        lock (InstallLock)
        {
            if (Installed.ContainsKey(signo))
            {
                return;
            }
            if (signo == 9 || signo == SigStop)
            {
                throw IpcException.Invalid($"signal {signo} cannot be caught");
            }
            var registration = PosixSignalRegistration.Create((PosixSignal)signo, context =>
            {
                context.Cancel = true;
                Wake.Bump();
            });
            Installed[signo] = registration;
            IpcTransports.Trace($"signal handler installed for signal {signo}");
        }
    }

    private static int OwnSignal(Endpoint endpoint)
    {
        var signo = (int)endpoint.Number("signo", 12, 1, 64);
        Install(signo);
        return signo;
    }

    private static int PeerSignal(IpcJson fields)
    {
        var signo = fields.RequireNumber("signal");
        if (signo is < 1 or > 64)
        {
            throw IpcException.InvalidData($"the peer asked for the invalid signal {signo}");
        }
        return (int)signo;
    }

    private sealed class SignalNotifier(uint peer, int peerSignal) : IRingNotifier
    {
        public ulong Generation => Wake.Generation;

        public void Notify() => PosixNative.Signal(peer, peerSignal);

        public void Wait(ulong generation, uint attempt, TimeSpan limit) => Wake.Wait(generation, attempt, limit);
    }

    private sealed class Setup : IRingSignalSetup
    {
        public IRingNotifier Finish(IpcJson ack, uint peer) => new SignalNotifier(peer, PeerSignal(ack));

        public void Dispose() { }
    }

    public static (IpcJson Fields, IRingSignalSetup Setup) Client(Endpoint endpoint) =>
        (new IpcJson().With("signal", (ulong)OwnSignal(endpoint)), new Setup());

    public static (IpcJson Fields, IRingNotifier Notifier) Server(Endpoint endpoint, IpcJson request, uint peer)
    {
        var own = OwnSignal(endpoint);
        var notifier = new SignalNotifier(peer, PeerSignal(request));
        return (new IpcJson().With("signal", (ulong)own), notifier);
    }
}
