using System.Text;

namespace Foundation.Data.Doublets.Cli;

/// <summary>
/// The append-only journal file <c>clink</c> keeps next to its database
/// (issue #195), and the one-time import of the older links-backed journal:
/// rust/src/transactions/journal_file.rs.
/// </summary>
/// <remarks>
/// Older versions stored each journal entry as the name of a fresh link in
/// <c>&lt;db&gt;.transitions.links</c> (plus
/// <c>&lt;db&gt;.transitions.names.links</c>). Every invocation re-read that
/// whole store, so each write got slower as the history grew. The journal is
/// now a plain text file, <c>&lt;db&gt;.transitions.log</c>, that a write
/// appends to. Opening the journal imports a legacy store once: its entries
/// are written to a temporary file that is synced and renamed into place
/// before the legacy files are removed, so a crash leaves either the legacy
/// store or a complete journal. An explicit <c>--transactions-file</c> that
/// still holds a links store is converted in place the same way.
/// </remarks>
public static class TransitionsJournal
{
    /// <summary>Every journal entry starts with this, so a journal file does too.</summary>
    private static readonly byte[] EntryPrefix = Encoding.ASCII.GetBytes("__transactions:");

    /// <summary>Conventional journal path: <c>&lt;stem&gt;.transitions.log</c> next to the database.</summary>
    public static string MakeTransitionsLogFilename(string databaseFilename) => Sibling(databaseFilename, "transitions.log");

    /// <summary>Path of the links-backed journal older versions kept: <c>&lt;stem&gt;.transitions.links</c>.</summary>
    public static string MakeLegacyTransitionsFilename(string databaseFilename) => Sibling(databaseFilename, "transitions.links");

    private static string Sibling(string path, string suffix)
    {
        ArgumentNullException.ThrowIfNull(path);
        var name = $"{Path.GetFileNameWithoutExtension(path)}.{suffix}";
        return Path.Combine(Path.GetDirectoryName(path) ?? string.Empty, name);
    }

    /// <summary>
    /// Opens the journal of <paramref name="database"/>: <paramref name="explicitPath"/>
    /// when given, the conventional <c>&lt;stem&gt;.transitions.log</c> otherwise,
    /// importing a legacy links-backed journal first.
    /// </summary>
    public static FileTransitionLog Open(string database, string? explicitPath, bool trace = false)
    {
        string path;
        if (!string.IsNullOrWhiteSpace(explicitPath))
        {
            if (IsLinksJournal(explicitPath)) ImportLinksJournal(explicitPath, explicitPath, trace);
            path = explicitPath;
        }
        else
        {
            path = MakeTransitionsLogFilename(database);
            var legacy = MakeLegacyTransitionsFilename(database);
            if (File.Exists(legacy)) ImportLinksJournal(legacy, path, trace);
        }
        return FileTransitionLog.Open(path);
    }

    /// <summary>
    /// Whether <paramref name="path"/> holds something other than a text
    /// journal: a non-empty file whose first bytes are not (a prefix of) the
    /// journal entry prefix.
    /// </summary>
    public static bool IsLinksJournal(string path)
    {
        if (!File.Exists(path)) return false;
        var head = new byte[EntryPrefix.Length];
        int length;
        using (var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete))
        {
            length = file.ReadAtLeast(head, head.Length, throwOnEndOfStream: false);
        }
        return length > 0 && !EntryPrefix.AsSpan(0, length).SequenceEqual(head.AsSpan(0, length));
    }

    /// <summary>
    /// Copies every entry of the links-backed journal at <paramref name="legacy"/>
    /// into the text journal at <paramref name="target"/>, then removes the
    /// legacy store. Returns whether anything was imported.
    /// </summary>
    /// <remarks>
    /// When <paramref name="target"/> already exists (a crash after
    /// publishing it), the legacy store is removed only if each of its
    /// entries is already in <paramref name="target"/>.
    /// </remarks>
    public static bool ImportLinksJournal(string legacy, string target, bool trace = false)
    {
        var names = NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(legacy);
        IReadOnlyList<string> entries;
        using (var store = new NamedTypesDecorator<uint>(legacy, trace))
        {
            entries = new NamedTypesTransitionLog<uint>(store, flush: null).ReadLogEntries();
        }
        var inPlace = string.Equals(Path.GetFullPath(legacy), Path.GetFullPath(target), StringComparison.Ordinal);
        if (!inPlace && File.Exists(target))
        {
            HashSet<string> present;
            using (var existing = FileTransitionLog.Open(target))
            {
                present = new HashSet<string>(existing.ReadLogEntries(), StringComparer.Ordinal);
            }
            if (entries.All(present.Contains))
            {
                File.Delete(legacy);
                File.Delete(names);
            }
            else if (trace)
            {
                DiagnosticOutput.WriteLine($"[Transactions] Kept legacy journal {legacy}: {target} already exists.");
            }
            return false;
        }
        var temporary = target + ".import";
        using (var file = new FileStream(temporary, FileMode.Create, FileAccess.Write, FileShare.None))
        {
            var contents = new StringBuilder();
            foreach (var entry in entries) contents.Append(entry).Append('\n');
            file.Write(new UTF8Encoding(false).GetBytes(contents.ToString()));
            file.Flush(flushToDisk: true);
        }
        File.Move(temporary, target, overwrite: true);
        if (!inPlace) File.Delete(legacy);
        File.Delete(names);
        FileTransitionLog.Open(target).Dispose(); // syncs the directory entries
        if (trace)
        {
            DiagnosticOutput.WriteLine($"[Transactions] Imported {entries.Count} entries from legacy journal {legacy} into {target}.");
        }
        return true;
    }
}
