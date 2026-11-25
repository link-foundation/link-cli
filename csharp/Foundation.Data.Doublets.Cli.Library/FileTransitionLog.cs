using System.Text;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli;

/// <summary>
/// Append-only, line-oriented transitions log backed by a single file:
/// rust/src/transactions/log.rs <c>FileTransitionLog</c>.
/// </summary>
/// <remarks>
/// <para>
/// Appending costs one write and one flush to stable storage regardless of
/// how many entries the log already holds. Retention policies compact it
/// through <see cref="CompactLogEntries"/>, which writes the retained entries
/// to <c>&lt;path&gt;.compact</c>, syncs it and renames it over the log, so
/// the journal on disk stays bounded (issue #195).
/// </para>
/// <para>
/// An interrupted append can leave a partial final line. Opening the log, or
/// reading, appending or flushing after a writer I/O error, repairs that
/// suffix before proceeding; trailing lines holding NUL, which power loss can
/// leave, are dropped too. A failed repair is retried on the next operation.
/// </para>
/// </remarks>
public sealed class FileTransitionLog : ITransitionLogStore, IDisposable
{
    private static readonly UTF8Encoding Utf8 = new(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);
    private const FileShare Sharing = FileShare.ReadWrite | FileShare.Delete;
    private FileStream _file;
    private bool _tailMayBeTorn;

    private FileTransitionLog(string path, FileStream file)
    {
        Path = path;
        _file = file;
    }

    /// <summary>Path of the backing file.</summary>
    public string Path { get; }

    /// <summary>Whether every append is flushed to stable storage. Enabled by default.</summary>
    public bool SyncOnAppend { get; set; } = true;

    public bool SupportsDurableFlush => true;

    public bool SupportsCompaction => true;

    /// <summary>
    /// Opens (creating if needed) the log at <paramref name="path"/>,
    /// discarding a trailing entry left half-written by a crash.
    /// </summary>
    public static FileTransitionLog Open(string path)
    {
        ArgumentException.ThrowIfNullOrEmpty(path);
        var parent = System.IO.Path.GetDirectoryName(path);
        if (!string.IsNullOrEmpty(parent)) Directory.CreateDirectory(parent);
        // A compaction interrupted before its rename leaves only a stale
        // copy; the log itself still holds the complete previous journal.
        File.Delete(CompactionPath(path));
        using (var repair = new FileStream(path, FileMode.OpenOrCreate, FileAccess.ReadWrite, Sharing))
        {
            TruncateTornTail(repair);
        }
        var file = OpenWriter(path);
        try
        {
            SyncParentDirectory(path);
        }
        catch
        {
            file.Dispose();
            throw;
        }
        return new FileTransitionLog(path, file);
    }

    private static FileStream OpenWriter(string path) => new(path, FileMode.Append, FileAccess.Write, Sharing);

    /// <summary>Temporary file a compaction writes before renaming it over the log.</summary>
    internal static string CompactionPath(string path) => path + ".compact";

    public void AppendLogEntry(string entry)
    {
        ArgumentNullException.ThrowIfNull(entry);
        if (entry.AsSpan().IndexOfAny('\n', '\r', '\0') >= 0)
            throw new ArgumentException("Transition log entries must not contain line breaks or NUL.", nameof(entry));
        RepairIfNeeded();
        _tailMayBeTorn = true;
        _file.Write(Utf8.GetBytes(entry + "\n"));
        _file.Flush(flushToDisk: SyncOnAppend);
        _tailMayBeTorn = false;
    }

    public IReadOnlyList<string> ReadLogEntries()
    {
        RepairIfNeeded();
        byte[] bytes;
        using (var reader = new FileStream(Path, FileMode.Open, FileAccess.Read, Sharing))
        {
            bytes = new byte[reader.Length];
            reader.ReadExactly(bytes);
        }
        var contents = Utf8.GetString(bytes);
        // Only newline-terminated lines were fully written; a trailing
        // fragment is the torn tail of a crashed append.
        var entries = new List<string>();
        var start = 0;
        for (var end = contents.IndexOf('\n'); end >= 0; start = end + 1, end = contents.IndexOf('\n', start))
        {
            var length = end - start;
            if (length > 0 && contents[end - 1] == '\r') length--;
            if (length > 0) entries.Add(contents.Substring(start, length));
        }
        return entries;
    }

    public void FlushLog()
    {
        RepairIfNeeded();
        _tailMayBeTorn = true;
        _file.Flush(flushToDisk: true);
        _tailMayBeTorn = false;
    }

    public void CompactLogEntries(IReadOnlyList<string> entries)
    {
        ArgumentNullException.ThrowIfNull(entries);
        foreach (var entry in entries)
        {
            if (string.IsNullOrEmpty(entry) || entry.AsSpan().IndexOfAny('\n', '\r', '\0') >= 0)
                throw new ArgumentException("Transition log entries must be non-empty single lines without NUL.", nameof(entries));
        }
        RepairIfNeeded();
        var temporary = CompactionPath(Path);
        var contents = new StringBuilder();
        foreach (var entry in entries) contents.Append(entry).Append('\n');
        try
        {
            using var written = new FileStream(temporary, FileMode.Create, FileAccess.Write, Sharing);
            written.Write(Utf8.GetBytes(contents.ToString()));
            written.Flush(flushToDisk: true);
        }
        catch
        {
            TryDelete(temporary);
            throw;
        }
        // Release the old handle first: Windows refuses to replace a file
        // that is still open. The rename publishes either the whole new
        // journal or nothing; both leave a complete journal behind. Until the
        // writer is reopened, repair reopens it first.
        _tailMayBeTorn = true;
        _file.Dispose();
        try
        {
            File.Move(temporary, Path, overwrite: true);
        }
        catch
        {
            TryDelete(temporary);
            throw;
        }
        SyncParentDirectory(Path);
        RepairIfNeeded();
    }

    public void Dispose() => _file.Dispose();

    // An error may follow successful short writes. Keep the flag until both
    // truncation and its durable flush succeed, so repeated I/O faults retry.
    private void RepairIfNeeded()
    {
        if (!_tailMayBeTorn) return;
        using (var repair = new FileStream(Path, FileMode.Open, FileAccess.ReadWrite, Sharing))
        {
            TruncateTornTail(repair);
            // A previous failed repair may already have changed the length.
            repair.Flush(flushToDisk: true);
        }
        // A failed compaction may have left the writer on a replaced file.
        var writer = OpenWriter(Path);
        _file.Dispose();
        _file = writer;
        _tailMayBeTorn = false;
    }

    private static void TryDelete(string path)
    {
        try { File.Delete(path); }
        catch (IOException) { /* the original error matters more */ }
        catch (UnauthorizedAccessException) { /* the original error matters more */ }
    }

    /// <summary>
    /// Makes a created or renamed directory entry durable (a no-op on
    /// Windows, where directories cannot be opened for syncing).
    /// </summary>
    private static void SyncParentDirectory(string path)
    {
        if (OperatingSystem.IsWindows()) return;
        var parent = System.IO.Path.GetDirectoryName(System.IO.Path.GetFullPath(path));
        FilePageDevice.SyncDirectory(string.IsNullOrEmpty(parent) ? "." : parent);
    }

    /// <summary>
    /// Shrinks <paramref name="file"/> to its last complete (newline-terminated)
    /// line that holds no NUL byte, scanning backwards in 8 KiB chunks.
    /// </summary>
    /// <remarks>
    /// Appending after a torn fragment would concatenate the next entry onto
    /// it. Power loss can also keep the newline of an entry while a sector
    /// before it reads back as zeros; entries never contain NUL, so trailing
    /// lines holding one are torn as well. A NUL before an intact line is
    /// damage that recovery reports instead.
    /// </remarks>
    internal static void TruncateTornTail(FileStream file)
    {
        var length = file.Length;
        if (length == 0) return;
        var end = length;
        // End of the line being scanned, just past its newline; -1 while
        // still in the fragment after the last newline. Blank lines are
        // skipped, as reading skips them.
        long complete = -1;
        var newerNewline = length;
        var zeroed = false;
        var buffer = new byte[8192];
        long keep;
        while (true)
        {
            if (end == 0)
            {
                // The first line of the file is reached.
                keep = zeroed ? 0 : Math.Max(complete, 0);
                break;
            }
            var chunk = (int)Math.Min(end, buffer.Length);
            var start = end - chunk;
            file.Seek(start, SeekOrigin.Begin);
            file.ReadExactly(buffer, 0, chunk);
            long found = -1;
            for (var offset = chunk - 1; offset >= 0; offset--)
            {
                var position = start + offset;
                var value = buffer[offset];
                if (value == (byte)'\n')
                {
                    if (complete < 0) complete = position + 1;
                    else if (newerNewline != position + 1)
                    {
                        if (!zeroed)
                        {
                            found = complete;
                            break;
                        }
                        complete = position + 1;
                        zeroed = false;
                    }
                    newerNewline = position;
                }
                else if (value == 0 && complete >= 0)
                {
                    zeroed = true;
                }
            }
            if (found >= 0)
            {
                keep = found;
                break;
            }
            end = start;
        }
        if (keep != length)
        {
            file.SetLength(keep);
            file.Flush(flushToDisk: true);
        }
    }
}
