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
        Unmap();
        _stream = new FileStream(_path, FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.Read);
        try
        {
            _stream.SetLength(newReservedCapacity);
            _mapping = MemoryMappedFile.CreateFromFile(_stream, null, newReservedCapacity,
                MemoryMappedFileAccess.ReadWrite, HandleInheritability.None, leaveOpen: true);
            _view = _mapping.CreateViewAccessor(0, newReservedCapacity);
            // The accessor remains owned until Unmap; offset zero is page aligned.
            Pointer = _view.SafeMemoryMappedViewHandle.DangerousGetHandle();
        }
        catch
        {
            Unmap();
            throw;
        }
    }

    private void Unmap()
    {
        _view?.Dispose();
        _view = null;
        _mapping?.Dispose();
        _mapping = null;
        _stream?.Dispose();
        _stream = null;
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
