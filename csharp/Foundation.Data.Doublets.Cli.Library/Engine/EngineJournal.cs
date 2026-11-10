using System.Numerics;
using System.Text;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>The first record that does not replay, and why.</summary>
public sealed record JournalDamage(int Record, string Reason);

/// <summary>What opening an engine would make of a journal.</summary>
/// <param name="ValidRecords">Records that replay, counted from the first; the valid prefix.</param>
/// <param name="TotalRecords">Records in the journal, damaged or not.</param>
/// <param name="Revision">Revision after the valid prefix; zero when nothing replays.</param>
/// <param name="Damage">Null when every record replays.</param>
public sealed record JournalCheck(int ValidRecords, int TotalRecords, ulong Revision, JournalDamage? Damage)
{
    public bool IsIntact => Damage is null;
    /// <summary>Records after the valid prefix that a repair from this copy drops.</summary>
    public int DiscardedRecords => TotalRecords - ValidRecords;
}

/// <summary>The journal a repair keeps: the longest valid prefix among the copies,
/// the index of the copy that supplied it (the first wins a tie), and one check per copy.</summary>
public sealed record JournalRecovery(IReadOnlyList<string> Entries, int Source, ulong Revision, IReadOnlyList<JournalCheck> Checks);

/// <summary>Outcome of <see cref="EngineJournal{T}.RepairFile"/>. <c>Changed</c> is false
/// when the file already held exactly the recovered journal; <c>Preserved</c> is
/// where the original bytes were kept before the file was replaced.</summary>
public sealed record JournalRepair(JournalRecovery Recovery, bool Changed, string? Preserved);

/// <summary>Check and repair a damaged engine journal. Opening an engine refuses a
/// journal with any bad record; these apply the same rules to report the longest
/// prefix that still replays, and to rebuild a journal from it or from an intact
/// copy. See docs/ENGINE-RECOVERY.md. The twin of Rust's <c>engine::recovery</c>.</summary>
public static class EngineJournal<T>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    private static readonly UTF8Encoding Lenient = new(false, false);

    /// <summary>Replay <paramref name="records"/> with the checksum, format, revision,
    /// allocator and selected constraint rules of opening. Never throws for damage:
    /// it is part of the report. Costs the same O(N) replay as opening, without a log.</summary>
    public static JournalCheck Check(IReadOnlyList<string> records, EngineOptions? options = null)
    {
        // History and durability do not change what replays.
        var engine = new LinksEngine<T>(new MemoryCommitLog(), (options ?? new()) with
        {
            HistoryLimit = 0,
            RequireDurableFlush = false,
            Trace = null,
        });
        for (int index = 0; index < records.Count; index++)
        {
            try { engine.Replay(records[index], index == 0); }
            catch (Exception error) when (error is not OutOfMemoryException)
            {
                return new(index, records.Count, engine.Current.Revision, new(index, error.Message));
            }
        }
        return new(records.Count, records.Count, engine.Current.Revision, null);
    }

    /// <summary>Pick the longest valid prefix among copies of one journal: the damaged
    /// primary first, then replicas, mirrors or a restored backup. Every other copy's
    /// valid prefix must be a prefix of the chosen one; copies that disagree are
    /// refused rather than guessed between, because each may hold commits the other
    /// lacks. A copy compacted into an engine3 checkpoint disagrees with an
    /// uncompacted copy of the same history.</summary>
    public static JournalRecovery Recover(IReadOnlyList<IReadOnlyList<string>> copies, EngineOptions? options = null)
    {
        if (copies.Count == 0) throw new ArgumentException("Journal recovery needs at least one copy.", nameof(copies));
        var checks = copies.Select(copy => Check(copy, options)).ToArray();
        int source = 0;
        for (int index = 1; index < checks.Length; index++)
            if (checks[index].ValidRecords > checks[source].ValidRecords) source = index;
        var chosen = copies[source].Take(checks[source].ValidRecords).ToArray();
        for (int index = 0; index < checks.Length; index++)
            for (int record = 0; record < checks[index].ValidRecords; record++)
                if (!string.Equals(copies[index][record], chosen[record], StringComparison.Ordinal))
                    throw new InvalidOperationException($"Journal copies {source} and {index} diverge; choose one explicitly.");
        return new(chosen, source, checks[source].Revision, checks);
    }

    /// <summary>Complete lines of a file journal, as <see cref="FileCommitLog"/> reads
    /// them, but tolerating damage it would reject: invalid UTF-8 becomes U+FFFD (so
    /// the checksum flags that record), blank lines are skipped, a torn final line and
    /// trailing lines with a NUL byte are ignored and a missing file is empty.</summary>
    public static IReadOnlyList<string> ReadFileLossy(string path) => Lines(ReadBytes(path));

    /// <summary>Rebuild the journal at <paramref name="path"/> from the longest valid
    /// prefix among it and <paramref name="copies"/> (see <see cref="Recover"/>). The
    /// original bytes are first copied to <c>&lt;path&gt;.damaged-&lt;n&gt;</c>; the
    /// replacement is flushed, then renamed over the path. Holds the writer lock that
    /// <see cref="FileCommitLog"/> takes, so it fails while an engine owns the journal.
    /// Copies are only read; their owners must not be writing them.</summary>
    public static JournalRepair RepairFile(string path, IReadOnlyList<string> copies, EngineOptions? options = null)
    {
        using var _ = LinksFileLock.TryAcquire(LinksFileLock.LockFilePath(path), LockMode.Exclusive)
            ?? throw new IOException("Journal writer is busy.");
        var original = ReadBytes(path);
        var sources = new List<IReadOnlyList<string>> { Lines(original) };
        sources.AddRange(copies.Select(ReadFileLossy));
        var recovery = Recover(sources, options);
        var repaired = new MemoryStream();
        foreach (var entry in recovery.Entries)
        {
            repaired.Write(Encoding.UTF8.GetBytes(entry));
            repaired.WriteByte((byte)'\n');
        }
        var bytes = repaired.ToArray();
        if (bytes.AsSpan().SequenceEqual(original)) return new(recovery, false, null);
        var preserved = original.Length == 0 ? null : Preserve(path, original);
        var temporary = path + ".repair";
        using (var file = new FileStream(temporary, FileMode.Create, FileAccess.Write, FileShare.None))
        {
            file.Write(bytes);
            file.Flush(flushToDisk: true);
        }
        File.Move(temporary, path, overwrite: true);
        if (!OperatingSystem.IsWindows()) FilePageDevice.SyncDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
        return new(recovery, true, preserved);
    }

    private static byte[] ReadBytes(string path)
    {
        try
        {
            using var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            using var buffer = new MemoryStream();
            file.CopyTo(buffer);
            return buffer.ToArray();
        }
        catch (Exception error) when (error is FileNotFoundException or DirectoryNotFoundException) { return Array.Empty<byte>(); }
    }

    private static IReadOnlyList<string> Lines(byte[] bytes)
    {
        var lines = new List<(int Start, int Length)>();
        int start = 0;
        for (int index = 0; index < bytes.Length; index++)
        {
            if (bytes[index] != (byte)'\n') continue;
            if (index > start) lines.Add((start, index - start));
            start = index + 1;
        }
        // Trailing lines with a NUL byte are torn by power loss, as FileCommitLog
        // drops them; one further back is damage.
        while (lines.Count > 0 && bytes.AsSpan(lines[^1].Start, lines[^1].Length).Contains((byte)0))
            lines.RemoveAt(lines.Count - 1);
        return lines.Select(line => Lenient.GetString(bytes, line.Start, line.Length)).ToList();
    }

    // Claim the first free <path>.damaged-<n> so no earlier evidence is lost.
    private static string Preserve(string path, byte[] bytes)
    {
        for (int attempt = 1; ; attempt++)
        {
            var target = $"{path}.damaged-{attempt}";
            FileStream file;
            try { file = new FileStream(target, FileMode.CreateNew, FileAccess.Write, FileShare.None); }
            catch (IOException) when (File.Exists(target)) { continue; }
            using (file)
            {
                file.Write(bytes);
                file.Flush(flushToDisk: true);
            }
            return target;
        }
    }
}
