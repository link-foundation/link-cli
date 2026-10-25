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

/// <summary>Exclusive file writer; repairs an incomplete final line on open
/// and before reuse after an I/O error. Complete records are preserved.</summary>
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

    private void RepairTail()
    {
        // Scan backwards in bounded chunks rather than loading a torn tail.
        long end = _file.Length;
        var buffer = new byte[8192];
        long complete = 0;
        while (end > 0)
        {
            int length = (int)Math.Min(end, buffer.Length);
            _file.Position = end - length;
            _file.ReadExactly(buffer.AsSpan(0, length));
            int offset = buffer.AsSpan(0, length).LastIndexOf((byte)'\n');
            if (offset >= 0) { complete = end - length + offset + 1; break; }
            end -= length;
        }
        if (complete != _file.Length) _file.SetLength(complete);
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
        if (record.Contains('\n') || record.Contains('\r'))
            throw new ArgumentException("Log records cannot contain line breaks.", nameof(record));
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
    public IReadOnlyList<string> Read() => _primary.Read();
    public void Append(string record) => _primary.Append(record);
    public void Flush()
    {
        _primary.Flush();
        if (_acknowledgements is { } required && Synchronize() < required)
            throw new IOException("Indeterminate replication outcome; reopen the engine.");
    }
    public int Synchronize()
    {
        _primary.Flush();
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
