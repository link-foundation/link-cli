namespace Foundation.Data.Doublets.Cli.Engine;

internal sealed class NativeLsmReadOnly : IRepairPageDevice, IDisposable
{
    private readonly FileStream?[] _files = new FileStream?[3];
    private readonly FileLsmIo _io;
    private bool _closed;
    private Exception? _failure;
    internal NativeLsmReadOnly(string path, FileLsmIo io)
    {
        _io = io;
        try
        {
            int index = 0;
            foreach (string name in FileLsmFiles.Parts(path))
            {
                if (NativeLsmFileType.Check(name, missing: true))
                { var file = io.OpenReader(name); _files[index] = file; NativeLsmFileType.CheckHandle(file); }
                index++;
            }
        }
        catch (Exception cause)
        { try { Dispose(); } catch (Exception cleanup) { throw new AggregateException("Native LSM read acquisition and cleanup failed.", cause, cleanup); } throw; }
    }
    private void Check() => ObjectDisposedException.ThrowIf(_closed, this);
    public long DataLength { get { Check(); return _files[0]?.Length ?? 0; } }
    public byte[]? ReadMetadata(int slot)
    {
        Check(); if (slot is not (0 or 1)) throw new ArgumentOutOfRangeException(nameof(slot));
        var file = _files[slot + 1]; if (file is null || file.Length == 0) return null;
        return file.Length == 128 ? Read(file, 128, 0) : new byte[1];
    }
    private static byte[] Read(FileStream file, int count, long offset)
    { byte[] bytes = new byte[count]; file.Position = offset; file.ReadExactly(bytes); return bytes; }
    public byte[] ReadPage(ulong index)
    {
        Check(); if (index >= PageFormat.MaxPages) throw new ArgumentOutOfRangeException(nameof(index));
        // Match a short physical read so a complete mirrored copy can supply it.
        return Read(_files[0] ?? throw new EndOfStreamException("Missing selected native LSM data."), 4096, checked((long)index * 4096));
    }
    private static Exception ReadOnly() => new InvalidOperationException("Read-only native LSM discovery.");
    public byte[] NewDatabaseId() => throw ReadOnly();
    public void WriteMetadata(int slot, ReadOnlySpan<byte> bytes) => throw ReadOnly();
    public void SyncMetadata(int slot) => throw ReadOnly();
    public void WritePage(ulong index, ReadOnlySpan<byte> bytes) => throw ReadOnly();
    public void RepairPage(ulong index, ReadOnlySpan<byte> bytes) => throw ReadOnly();
    public void SyncData() => throw ReadOnly();
    public void TruncateData(long length) => throw ReadOnly();
    public void Dispose()
    {
        if (_closed) { if (_failure is not null) throw _failure; return; }
        _closed = true;
        var errors = new List<Exception>();
        foreach (var file in _files.Reverse()) if (file is not null) try { _io.Close(file); } catch (Exception error) { errors.Add(error); }
        if (errors.Count != 0) { _failure = new AggregateException("Native LSM discovery cleanup failed.", errors); throw _failure; }
    }
}
internal sealed class NativeLsmDeferred(IPageDevice device) : IPageDevice
{
    public bool SupportsDurableFlush => device.SupportsDurableFlush;
    public uint MetadataFlags => device.MetadataFlags;
    public long DataLength => device.DataLength;
    public byte[]? ReadMetadata(int slot) => device.ReadMetadata(slot);
    public byte[] ReadPage(ulong index) => device.ReadPage(index);
    public byte[] ReadCheckedPage(ulong index, ulong expected) => device.ReadCheckedPage(index, expected);
    private static Exception ReadOnly() => new InvalidOperationException("Read-only native LSM table.");
    public byte[] NewDatabaseId() => throw ReadOnly();
    public void WriteMetadata(int slot, ReadOnlySpan<byte> bytes) => throw ReadOnly();
    public void SyncMetadata(int slot) => throw ReadOnly();
    public void WritePage(ulong index, ReadOnlySpan<byte> bytes) => throw ReadOnly();
    public void SyncData() => throw ReadOnly();
    public void TruncateData(long length) => throw ReadOnly();
}
internal sealed class NativeLsmReadOnlyWal(IReadOnlyList<string> records) : IReplaceableCommitLog
{
    public bool SupportsDurableFlush => false;
    public bool SupportsReplacement => true;
    public IReadOnlyList<string> Read() => records.ToArray();
    private static Exception ReadOnly() => new InvalidOperationException("Read-only native LSM WAL.");
    public void Append(string record) => throw ReadOnly();
    public void Flush() => throw ReadOnly();
    public bool Replace(IReadOnlyList<string> replacement) => throw ReadOnly();
}
