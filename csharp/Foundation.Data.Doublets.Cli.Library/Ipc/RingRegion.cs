using System.Diagnostics.CodeAnalysis;
using System.Buffers.Binary;
using System.IO.MemoryMappedFiles;
using Microsoft.Win32.SafeHandles;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>Bytes both processes of a ring connection see.</summary>
internal interface IRingRegion : IDisposable
{
    long Length { get; }

    uint Load(long offset);

    void Store(long offset, uint value);

    void ReadBytes(long offset, Span<byte> buffer);

    void WriteBytes(long offset, ReadOnlySpan<byte> data);
}

/// <summary>Shared memory mapped into this process, released on dispose.</summary>
internal sealed unsafe class MappedRegion : IRingRegion
{
    private readonly byte* _pointer;
    private Action? _release;

    public MappedRegion(byte* pointer, long length, Action release)
    {
        _pointer = pointer;
        Length = length;
        _release = release;
    }

    /// <summary>Maps <paramref name="length"/> bytes of an open file, which the region then owns.</summary>
    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static MappedRegion OfFile(FileStream file, long length)
    {
        var mapped = MemoryMappedFile.CreateFromFile(file, null, length, MemoryMappedFileAccess.ReadWrite,
            HandleInheritability.None, leaveOpen: false);
        return OfMapping(mapped, length);
    }

    /// <summary>Maps a whole memory-mapped file object; a zero length takes the size of the view.</summary>
    public static MappedRegion OfMapping(MemoryMappedFile mapped, long length)
    {
        MemoryMappedViewAccessor? view = null;
        try
        {
            view = mapped.CreateViewAccessor(0, length, MemoryMappedFileAccess.ReadWrite);
            byte* pointer = null;
            view.SafeMemoryMappedViewHandle.AcquirePointer(ref pointer);
            var owned = view;
            return new MappedRegion(pointer + view.PointerOffset, length == 0 ? view.Capacity : length, () =>
            {
                owned.SafeMemoryMappedViewHandle.ReleasePointer();
                owned.Dispose();
                mapped.Dispose();
            });
        }
        catch
        {
            view?.Dispose();
            mapped.Dispose();
            throw;
        }
    }

    public long Length { get; }

    private uint* Word(long offset)
    {
        if (offset % 4 != 0 || offset < 0 || offset + 4 > Length)
        {
            throw new ArgumentOutOfRangeException(nameof(offset));
        }
        return (uint*)(_pointer + offset);
    }

    public uint Load(long offset)
    {
        var value = Volatile.Read(ref *Word(offset));
        return BitConverter.IsLittleEndian ? value : BinaryPrimitives.ReverseEndianness(value);
    }

    public void Store(long offset, uint value) =>
        Volatile.Write(ref *Word(offset), BitConverter.IsLittleEndian ? value : BinaryPrimitives.ReverseEndianness(value));

    public void ReadBytes(long offset, Span<byte> buffer)
    {
        Check(offset, buffer.Length);
        new ReadOnlySpan<byte>(_pointer + offset, buffer.Length).CopyTo(buffer);
        Thread.MemoryBarrier();
    }

    public void WriteBytes(long offset, ReadOnlySpan<byte> data)
    {
        Check(offset, data.Length);
        data.CopyTo(new Span<byte>(_pointer + offset, data.Length));
        Thread.MemoryBarrier();
    }

    private void Check(long offset, int count)
    {
        if (offset < 0 || offset + count > Length)
        {
            throw new ArgumentOutOfRangeException(nameof(offset));
        }
    }

    public void Dispose() => Interlocked.Exchange(ref _release, null)?.Invoke();
}

/// <summary>A regular file accessed only with positioned reads and writes (the <c>file</c> kind).</summary>
internal sealed class FileRegion(SafeFileHandle file, long length) : IRingRegion
{
    public long Length { get; } = length;

    public uint Load(long offset)
    {
        Span<byte> bytes = stackalloc byte[4];
        ReadBytes(offset, bytes);
        return BinaryPrimitives.ReadUInt32LittleEndian(bytes);
    }

    public void Store(long offset, uint value)
    {
        Span<byte> bytes = stackalloc byte[4];
        BinaryPrimitives.WriteUInt32LittleEndian(bytes, value);
        WriteBytes(offset, bytes);
    }

    public void ReadBytes(long offset, Span<byte> buffer)
    {
        while (!buffer.IsEmpty)
        {
            var read = RandomAccess.Read(file, buffer, offset);
            if (read == 0)
            {
                throw new EndOfStreamException("the ring file is shorter than its header says");
            }
            buffer = buffer[read..];
            offset += read;
        }
    }

    public void WriteBytes(long offset, ReadOnlySpan<byte> data) => RandomAccess.Write(file, data, offset);

    public void Dispose() => file.Dispose();
}

/// <summary>How a ring side waits for the other one.</summary>
internal interface IRingNotifier : IDisposable
{
    /// <summary>A value that changes on every wake-up; read before checking the ring.</summary>
    ulong Generation => 0;

    /// <summary>Wakes the peer after this side published something.</summary>
    void Notify() { }

    /// <summary>Waits for a wake-up after <paramref name="generation"/>, at most <paramref name="limit"/>.</summary>
    void Wait(ulong generation, uint attempt, TimeSpan limit);

    void IDisposable.Dispose() { }
}

/// <summary>Polling with a backoff from busy checks to sleeps of at most the poll interval.</summary>
internal sealed class PollNotifier(TimeSpan poll) : IRingNotifier
{
    public void Wait(ulong generation, uint attempt, TimeSpan limit)
    {
        if (attempt < 64)
        {
            Thread.SpinWait(1);
        }
        else if (attempt < 128)
        {
            Thread.Yield();
        }
        else
        {
            var step = TimeSpan.FromTicks(500L << (int)Math.Min(attempt - 128, 10));
            var sleep = step < poll ? step : poll;
            Thread.Sleep(sleep < limit ? sleep : limit);
        }
    }
}

/// <summary>Wakes every waiter of a process-wide generation counter.</summary>
internal sealed class GenerationWake
{
    private readonly object _lock = new();
    private ulong _generation;

    public ulong Generation => Interlocked.Read(ref _generation);

    public void Bump()
    {
        lock (_lock)
        {
            Interlocked.Increment(ref _generation);
            Monitor.PulseAll(_lock);
        }
    }

    /// <summary>The notifier wait of the <c>signal</c> kind: spin briefly, then wait for a bump.</summary>
    public void Wait(ulong generation, uint attempt, TimeSpan limit)
    {
        if (attempt < 16)
        {
            Thread.SpinWait(1);
            return;
        }
        lock (_lock)
        {
            if (Generation != generation)
            {
                return;
            }
            Monitor.Wait(_lock, limit);
        }
    }
}
