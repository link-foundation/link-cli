using System.IO.MemoryMappedFiles;
using Platform.Memory;

namespace Foundation.Data.Doublets.Cli;

/// <summary>A mapping with an owned file handle for ordered, durable flushes.</summary>
/// <remarks>Flush the view before the file. Reopening Platform.Memory's mapping
/// for write access fails Windows sharing checks and does not flush its view.</remarks>
internal sealed class DurableMappedMemory : ResizableDirectMemoryBase
{
    private readonly string _path;
    private FileStream? _stream;
    private MemoryMappedFile? _mapping;
    private MemoryMappedViewAccessor? _view;
    protected override string ObjectName => _path;

    internal DurableMappedMemory(string path, long capacity)
    {
        _path = path;
        var size = File.Exists(path) ? new FileInfo(path).Length : 0;
        capacity = Math.Max(capacity, MinimumCapacity);
        ReservedCapacity = size > capacity ? checked((size / capacity + 1) * capacity) : capacity;
        UsedCapacity = size;
    }

    internal void Flush()
    {
        ObjectDisposedException.ThrowIf(_view is null || _stream is null, this);
        _view.Flush();
        _stream.Flush(flushToDisk: true);
    }

    protected override void OnReservedCapacityChanged(long oldReservedCapacity, long newReservedCapacity)
    {
        // Keep both the live pointer and file ownership until a replacement is
        // ready. The base retains its old capacity when this callback throws.
        var stream = _stream ?? new FileStream(_path, FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.Read);
        MemoryMappedFile? mapping = null;
        MemoryMappedViewAccessor? view = null;
        try
        {
            // CreateFromFile grows the backing file itself. Explicit SetLength
            // requires unmapping first on Windows. Smaller reservations change
            // the view; the file is truncated to UsedCapacity during disposal.
            mapping = MemoryMappedFile.CreateFromFile(stream, null, Math.Max(newReservedCapacity, stream.Length),
                MemoryMappedFileAccess.ReadWrite, HandleInheritability.None, leaveOpen: true);
            view = mapping.CreateViewAccessor(0, newReservedCapacity);
        }
        catch
        {
            view?.Dispose();
            mapping?.Dispose();
            if (_stream is null) stream.Dispose();
            throw;
        }
        var oldView = _view;
        var oldMapping = _mapping;
        _stream = stream;
        _mapping = mapping;
        _view = view;
        // The accessor owns this offset-zero, page-aligned pointer until the
        // next successful replacement or disposal.
        Pointer = view.SafeMemoryMappedViewHandle.DangerousGetHandle();
        oldView?.Dispose();
        oldMapping?.Dispose();
    }

    protected override void DisposePointer(IntPtr pointer, long usedCapacity)
    {
        _view?.Dispose();
        _view = null;
        _mapping?.Dispose();
        _mapping = null;
        try { _stream?.SetLength(usedCapacity); }
        finally { _stream?.Dispose(); _stream = null; }
    }
}
