using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using Microsoft.Win32.SafeHandles;

namespace Foundation.Data.Doublets.Cli;

/// <summary>
/// The operating system lock <c>--lock flock</c> takes on <c>&lt;db&gt;.lock</c>
/// (issue #194), the same lock the Rust and JavaScript <c>clink</c> take, so
/// that every implementation excludes the others: <c>flock(2)</c> on Unix and
/// <c>LockFileEx</c> over the whole file (offset 0, length <c>u32::MAX</c>
/// twice) on Windows. The system releases it when the holder exits.
/// </summary>
/// <remarks>
/// On Unix the runtime takes the <c>flock</c> itself when a file is opened:
/// <see cref="FileShare.None"/> is <c>LOCK_EX</c>, anything else
/// <c>LOCK_SH</c> (see <see cref="LinksFileLock"/>). On Windows
/// <see cref="FileShare"/> only sets share modes, which the Rust and
/// JavaScript <c>clink</c> do not see, so the lock is taken with
/// <c>LockFileEx</c> on a handle that shares everything.
/// </remarks>
public sealed class StoreFileLock : IDisposable
{
    private readonly IDisposable _held;

    private StoreFileLock(IDisposable held, string path, LockMode mode)
    {
        _held = held;
        Path = path;
        Mode = mode;
    }

    /// <summary>The lock file.</summary>
    public string Path { get; }

    /// <summary>Whether the lock is shared or exclusive.</summary>
    public LockMode Mode { get; }

    /// <summary>Tries the lock once; <see langword="null"/> while another holder has a conflicting one.</summary>
    public static StoreFileLock? TryAcquire(string lockPath, LockMode mode)
    {
        ArgumentNullException.ThrowIfNull(lockPath);
        IDisposable? held = OperatingSystem.IsWindows()
          ? WindowsLock.TryAcquire(lockPath, mode)
          : LinksFileLock.TryAcquire(lockPath, mode);
        try
        {
            if (held is null) return null;
            var acquired = new StoreFileLock(held, lockPath, mode);
            held = null;
            return acquired;
        }
        finally
        {
            held?.Dispose();
        }
    }

    /// <summary>Releases the lock.</summary>
    public void Dispose() => _held.Dispose();

    [SupportedOSPlatform("windows")]
    private sealed class WindowsLock : IDisposable
    {
        private const uint LockfileFailImmediately = 0x1;
        private const uint LockfileExclusiveLock = 0x2;
        private const int ErrorLockViolation = 33;
        private const int ErrorIoPending = 997;

        private readonly FileStream _stream;

        private WindowsLock(FileStream stream) => _stream = stream;

        public static WindowsLock? TryAcquire(string lockPath, LockMode mode)
        {
            var directory = System.IO.Path.GetDirectoryName(lockPath);
            if (!string.IsNullOrEmpty(directory)) Directory.CreateDirectory(directory);
            FileStream? stream = new FileStream(
              lockPath, FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.ReadWrite | FileShare.Delete);
            try
            {
                var flags = LockfileFailImmediately | (mode == LockMode.Exclusive ? LockfileExclusiveLock : 0);
                var overlapped = default(NativeOverlapped);
                if (LockFileEx(stream.SafeFileHandle, flags, 0, uint.MaxValue, uint.MaxValue, ref overlapped))
                {
                    var held = new WindowsLock(stream);
                    stream = null;
                    return held;
                }
                var error = Marshal.GetLastPInvokeError();
                if (error is ErrorLockViolation or ErrorIoPending) return null;
                throw new IOException($"Cannot lock '{lockPath}'.", new Win32Exception(error));
            }
            finally
            {
                stream?.Dispose();
            }
        }

        public void Dispose()
        {
            // Closing the handle releases the lock as well, but possibly later.
            _ = UnlockFile(_stream.SafeFileHandle, 0, 0, uint.MaxValue, uint.MaxValue);
            _stream.Dispose();
        }

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool LockFileEx(
          SafeFileHandle file, uint flags, uint reserved, uint lengthLow, uint lengthHigh, ref NativeOverlapped overlapped);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool UnlockFile(
          SafeFileHandle file, uint offsetLow, uint offsetHigh, uint lengthLow, uint lengthHigh);
    }
}
