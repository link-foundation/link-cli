using System.Text;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Replaceable, ordered revision log. One engine owns its writer.</summary>
public interface ICommitLog
{
    bool SupportsDurableFlush { get; }
    IReadOnlyList<string> Read();
    void Append(string record);
    void Flush();
}

public sealed class MemoryCommitLog : ICommitLog
{
    private readonly List<string> _records = new();
    public bool SupportsDurableFlush => false;
    public IReadOnlyList<string> Read() => _records.ToArray();
    public void Append(string record) => _records.Add(record);
    public void Flush() { }
}

/// <summary>Exclusive file writer; repairs an incomplete final line, or trailing
/// lines with a NUL byte that power loss left, on open and before reuse after an
/// I/O error. Complete records are preserved.</summary>
public sealed class FileCommitLog : ICommitLog, IDisposable
{
    private readonly FileStream _file;
    private readonly LinksFileLock _lock;
    private bool _tailMayBeTorn;
    private static readonly UTF8Encoding Utf8 = new(false, true);
    public bool SupportsDurableFlush => true;

    public FileCommitLog(string path)
        : this(path, file => new FileStream(file, FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.Read, bufferSize: 1)) { }

    // The same owned file protocol can be exercised with finite syscall faults.
    internal FileCommitLog(string path, Func<string, FileStream> open)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
        _lock = LinksFileLock.TryAcquire(LinksFileLock.LockFilePath(path), LockMode.Exclusive)
            ?? throw new IOException("Journal writer is busy.");
        try { _file = open(path); }
        catch { _lock.Dispose(); throw; }
        try { RepairTail(); }
        catch { _file.Dispose(); _lock.Dispose(); throw; }
    }

    // Power loss can also keep the newline of a record that was never flushed
    // while a sector before it reads back as zeros (the disk reordered writes no
    // flush ordered). Records never contain NUL (Append refuses it), so trailing
    // lines holding one are torn too, as etcd's WAL treats a last record with a
    // zeroed sector. A NUL before an intact line is damage that reading refuses.
    private void RepairTail()
    {
        // Scan backwards in bounded chunks rather than loading a torn tail.
        long end = _file.Length;
        var buffer = new byte[8192];
        long? complete = null; // End of the line being scanned, past its newline.
        long newerNewline = end;
        bool zeroed = false;
        long keep = -1;
        while (keep < 0)
        {
            if (end == 0) { keep = zeroed ? 0 : complete ?? 0; break; }
            int length = (int)Math.Min(end, buffer.Length);
            long start = end - length;
            _file.Position = start;
            _file.ReadExactly(buffer.AsSpan(0, length));
            for (int offset = length - 1; offset >= 0 && keep < 0; offset--)
            {
                long position = start + offset;
                byte value = buffer[offset];
                if (value == (byte)'\n')
                {
                    // Blank lines are skipped, as reading the journal skips them.
                    if (complete is null) complete = position + 1;
                    else if (newerNewline != position + 1)
                    {
                        if (!zeroed) keep = complete.Value;
                        else { complete = position + 1; zeroed = false; }
                    }
                    newerNewline = position;
                }
                else if (value == 0 && complete is not null) zeroed = true;
            }
            end = start;
        }
        if (keep != _file.Length) _file.SetLength(keep);
        // Retry persistence even if an earlier failed repair changed the length.
        _file.Flush(flushToDisk: true);
        _file.Position = _file.Length;
    }

    private void RepairIfNeeded()
    {
        if (!_tailMayBeTorn) return;
        RepairTail();
        _tailMayBeTorn = false;
    }

    public IReadOnlyList<string> Read()
    {
        RepairIfNeeded();
        _file.Position = 0;
        using var reader = new StreamReader(_file, Utf8, false, 8192, leaveOpen: true);
        var records = new List<string>();
        while (reader.ReadLine() is { } line) records.Add(line);
        _file.Position = _file.Length;
        return records;
    }
    public void Append(string record)
    {
        if (record.AsSpan().IndexOfAny('\n', '\r', '\0') >= 0)
            throw new ArgumentException("Log records cannot contain line breaks or NUL.", nameof(record));
        var bytes = Utf8.GetBytes(record + "\n");
        RepairIfNeeded();
        _file.Position = _file.Length;
        // No managed write buffer: after a failed syscall it must not reinsert
        // an old fragment while seeking or truncating for recovery.
        _tailMayBeTorn = true;
        _file.Write(bytes);
        _tailMayBeTorn = false;
    }
    public void Flush()
    {
        RepairIfNeeded();
        _tailMayBeTorn = true;
        _file.Flush(flushToDisk: true);
        _tailMayBeTorn = false;
    }
    public void Dispose() { _file.Dispose(); _lock.Dispose(); }
}

/// <summary>Fixed-primary prefix replication. This does not elect a primary.</summary>
public sealed class ReplicatedCommitLog : ICommitLog
{
    private readonly ICommitLog _primary;
    private readonly ICommitLog[] _replicas;
    private readonly int? _acknowledgements;
    public Action<string>? Trace { get; init; }

    /// <param name="primary">Authoritative log owned by this writer.</param>
    /// <param name="replicas">Fixed follower membership.</param>
    /// <param name="acknowledgements">Null for caller-scheduled asynchronous catch-up;
    /// otherwise the required copies including the primary.</param>
    public ReplicatedCommitLog(ICommitLog primary, IEnumerable<ICommitLog> replicas, int? acknowledgements = null)
    {
        _primary = primary;
        _replicas = replicas.ToArray();
        if (_replicas.Any(replica => ReferenceEquals(replica, primary)) ||
            _replicas.Distinct(ReferenceEqualityComparer.Instance).Count() != _replicas.Length)
            throw new ArgumentException("Replica membership must contain distinct log instances.", nameof(replicas));
        if (acknowledgements is <= 0 || acknowledgements > _replicas.Length + 1)
            throw new ArgumentOutOfRangeException(nameof(acknowledgements));
        _acknowledgements = acknowledgements;
    }
    public bool SupportsDurableFlush => _primary.SupportsDurableFlush &&
        (_acknowledgements is null or 1 || _replicas.All(log => log.SupportsDurableFlush));
    /// <summary>Reads the primary, refusing it when a replica holds its whole log and more.
    /// Replicas only receive records the primary has flushed, so that shape means the primary
    /// lost acknowledged commits (a truncated file, an old backup); writing on top would fork
    /// history and drop them for good. Unreadable or divergent replicas stay ignored.</summary>
    public IReadOnlyList<string> Read()
    {
        var records = _primary.Read();
        for (int i = 0; i < _replicas.Length; i++)
        {
            IReadOnlyList<string> copy;
            try { copy = _replicas[i].Read(); }
            catch (Exception error) when (error is IOException or InvalidDataException)
            { EngineDiagnostics.Trace(Trace, error.Message); continue; }
            if (copy.Count > records.Count && copy.Take(records.Count).SequenceEqual(records))
                throw new InvalidDataException(
                    $"Replica {i} holds {copy.Count - records.Count} records the primary lacks; repair the primary from it before writing.");
        }
        return records;
    }
    public void Append(string record) => _primary.Append(record);
    public void Flush()
    {
        _primary.Flush();
        if (_acknowledgements is { } required && SynchronizeFlushed() < required)
            throw new IOException("Indeterminate replication outcome; reopen the engine.");
    }
    public int Synchronize()
    {
        _primary.Flush();
        return SynchronizeFlushed();
    }
    private int SynchronizeFlushed()
    {
        var records = _primary.Read();
        int count = 1;
        foreach (var replica in _replicas)
        {
            try
            {
                var previous = replica.Read();
                if (previous.Count > records.Count || !records.Take(previous.Count).SequenceEqual(previous))
                    throw new InvalidDataException("Replica diverges from primary.");
                foreach (var record in records.Skip(previous.Count)) replica.Append(record);
                replica.Flush();
                count++;
            }
            catch (Exception error) when (error is IOException or InvalidDataException)
            { EngineDiagnostics.Trace(Trace, error.Message); }
        }
        return count;
    }
}
