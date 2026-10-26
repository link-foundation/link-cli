using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using static Foundation.Data.Doublets.Cli.Engine.PageFormat;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Native exclusive page I/O, independent of SQL. Parent must exist.
/// Atomic .writer ownership is never stolen. After a close error the ownership
/// file remains; externally fence the old process before explicit removal.</summary>
public sealed class FilePageDevice : IRepairPageDevice, IDisposable
{
    private readonly List<FileStream> _files = new();
    private readonly FileStream _owner;
    private readonly string _lock;
    private bool _closed;
    public bool SupportsDurableFlush => true;

    public FilePageDevice(string path) : this(path, OpenFile, SyncDirectory) { }
    internal FilePageDevice(string path, Func<string, bool, FileStream> open, Action<string> syncDirectory)
    {
        path = Path.GetFullPath(path); _lock = path + ".writer";
        _owner = open(_lock, true);
        try
        {
            foreach (string suffix in new[] { "", ".meta0", ".meta1" }) _files.Add(open(path + suffix, false));
            if (!OperatingSystem.IsWindows()) syncDirectory(Path.GetDirectoryName(path)!);
        }
        catch (Exception cause)
        {
            try { Dispose(); }
            catch (Exception cleanup) { throw new AggregateException("Native page initialization and cleanup failed.", cause, cleanup); }
            throw;
        }
    }
    private static FileStream OpenFile(string path, bool owner)
    {
        var options = new FileStreamOptions
        {
            Mode = owner ? FileMode.CreateNew : FileMode.OpenOrCreate,
            Access = FileAccess.ReadWrite,
            Share = FileShare.Read,
            BufferSize = 1
        };
        if (!OperatingSystem.IsWindows()) options.UnixCreateMode = UnixFileMode.UserRead | UnixFileMode.UserWrite;
        return new FileStream(path, options);
    }
    private FileStream File(int index)
    {
        ObjectDisposedException.ThrowIf(_closed, this); return _files[index];
    }
    private static int Slot(int slot) => slot is 0 or 1 ? slot + 1 : throw Invalid("metadata slot");
    public byte[] NewDatabaseId() { _ = File(0); return RandomNumberGenerator.GetBytes(16); }
    public long DataLength => File(0).Length;
    public byte[]? ReadMetadata(int slot)
    {
        var file = File(Slot(slot));
        if (file.Length == 0) return null;
        if (file.Length != 128) return new byte[1]; // Invalid sentinel; no unbounded allocation.
        var bytes = new byte[128]; file.Position = 0; file.ReadExactly(bytes); return bytes;
    }
    public void WriteMetadata(int slot, ReadOnlySpan<byte> bytes)
    {
        if (bytes.Length != 128) throw Invalid("metadata length");
        var file = File(Slot(slot)); file.Position = 0; file.Write(bytes); file.SetLength(128);
    }
    public void SyncMetadata(int slot) => File(Slot(slot)).Flush(flushToDisk: true);
    private static long Position(ulong index) => index < MaxPages ? checked((long)index * PageSize) : throw Invalid("physical page address");
    public byte[] ReadPage(ulong index)
    {
        long position = Position(index); var file = File(0); var bytes = new byte[PageSize];
        file.Position = position; file.ReadExactly(bytes); return bytes;
    }
    public void WritePage(ulong index, ReadOnlySpan<byte> bytes)
    {
        if (bytes.Length != PageSize) throw Invalid("page length");
        long position = Position(index); var file = File(0);
        if (file.Length != position) throw Invalid("native page file changed during ownership");
        file.Position = position; file.Write(bytes);
    }
    public void RepairPage(ulong index, ReadOnlySpan<byte> bytes)
    {
        if (bytes.Length != PageSize) throw Invalid("repair page length");
        long position = Position(index); var file = File(0);
        if (file.Length < position) throw Invalid("repair would create a hole");
        file.Position = position; file.Write(bytes);
    }
    public void SyncData() => File(0).Flush(flushToDisk: true);
    public void TruncateData(long length)
    {
        if (length < 0 || (ulong)length > MaxPages * PageSize) throw Invalid("physical data length");
        File(0).SetLength(length);
    }
    public void Dispose()
    {
        if (_closed) return; _closed = true;
        var failures = new List<Exception>();
        foreach (var file in _files.AsEnumerable().Reverse().Append(_owner))
        {
            try { file.Dispose(); } catch (Exception error) { failures.Add(error); }
        }
        _files.Clear();
        // A failed close can already have released its OS handle. Never retry
        // an uncertain close or expose new ownership while a handle may be live.
        if (failures.Count == 0)
        {
            try { System.IO.File.Delete(_lock); } catch (Exception error) { failures.Add(error); }
        }
        if (failures.Count > 0) throw new AggregateException("Native page cleanup failed; ownership remains fenced.", failures);
    }
    private static void SyncDirectory(string path)
    {
        int descriptor = OpenDirectory(path, 0);
        if (descriptor < 0) throw new IOException("Cannot open native page parent directory.", new Win32Exception(Marshal.GetLastPInvokeError()));
        Exception? failure = null;
        if (Fsync(descriptor) != 0) failure = new IOException("Cannot synchronize native page parent directory.", new Win32Exception(Marshal.GetLastPInvokeError()));
        // One close only: an error can still have released this descriptor.
        if (Close(descriptor) != 0)
        {
            var close = new IOException("Cannot close native page parent directory.", new Win32Exception(Marshal.GetLastPInvokeError()));
            failure = failure is null ? close : new AggregateException(failure, close);
        }
        if (failure is not null) throw failure;
    }
    [DllImport("libc", EntryPoint = "open", SetLastError = true)]
    private static extern int OpenDirectory([MarshalAs(UnmanagedType.LPUTF8Str)] string path, int flags);
    [DllImport("libc", EntryPoint = "fsync", SetLastError = true)]
    private static extern int Fsync(int descriptor);
    [DllImport("libc", EntryPoint = "close", SetLastError = true)]
    private static extern int Close(int descriptor);
}
