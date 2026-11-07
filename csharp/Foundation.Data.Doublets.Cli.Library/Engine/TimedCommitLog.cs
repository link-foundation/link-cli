using System.Globalization;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Optional log decorator that keeps <c>revision|unix milliseconds</c> in a
/// second log for every appended record, so a restore can name a time. Records and
/// their format are unchanged. The record is written and flushed first, so a crash
/// can lose a timestamp but never leave one for a record that is not there. Time
/// never runs backwards: a clock that steps back repeats the latest time.</summary>
public sealed class TimedCommitLog : IReplaceableCommitLog
{
    private readonly ICommitLog _inner;
    private readonly ICommitLog _timeline;
    private readonly Func<ulong> _clock;
    private ulong _latest;

    public TimedCommitLog(ICommitLog inner, ICommitLog timeline, Func<ulong>? clock = null)
    {
        ArgumentNullException.ThrowIfNull(inner);
        ArgumentNullException.ThrowIfNull(timeline);
        _inner = inner;
        _timeline = timeline;
        _clock = clock ?? (() => (ulong)DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());
        var entries = Surviving(timeline);
        _latest = entries.Count == 0 ? 0 : entries[^1].Written;
    }

    public bool SupportsDurableFlush => _inner.SupportsDurableFlush && _timeline.SupportsDurableFlush;
    public bool SupportsReplacement => _inner is IReplaceableCommitLog { SupportsReplacement: true };
    public IReadOnlyList<string> Read() => _inner.Read();

    public void Append(string record)
    {
        ArgumentNullException.ThrowIfNull(record);
        var revision = RevisionOf(record);
        _inner.Append(record);
        _latest = Math.Max(_latest, _clock());
        _timeline.Append(string.Create(CultureInfo.InvariantCulture, $"{revision}|{_latest}"));
    }

    public void Flush()
    {
        _inner.Flush();
        _timeline.Flush();
    }

    /// <summary>Compaction leaves the timeline alone: its checkpoint repeats a revision
    /// the timeline holds, and a restore refuses the revisions it dropped.</summary>
    public bool Replace(IReadOnlyList<string> records)
    {
        if (_inner is not IReplaceableCommitLog { SupportsReplacement: true } replacement)
            throw new NotSupportedException("Selected log does not support journal replacement.");
        return replacement.Replace(records);
    }

    /// <summary>The latest revision written by <paramref name="time"/> (unix
    /// milliseconds), for <c>RestoreBackup</c>. Refuses a time before the first
    /// timestamp, because the state then is not known.</summary>
    public static ulong RevisionAt(ICommitLog timeline, ulong time)
    {
        ArgumentNullException.ThrowIfNull(timeline);
        var entries = Surviving(timeline);
        for (var index = entries.Count - 1; index >= 0; index--)
            if (entries[index].Written <= time) return entries[index].Revision;
        throw new InvalidOperationException("No revision was written by that time.");
    }

    private static ulong RevisionOf(string record)
    {
        var fields = record.Split('|');
        if (!record.StartsWith("engine", StringComparison.Ordinal) || fields.Length < 2
            || !ulong.TryParse(fields[1], NumberStyles.None, CultureInfo.InvariantCulture, out var revision))
            throw new InvalidDataException("Cannot timestamp a record without a revision.");
        return revision;
    }

    // A revision written again after a crash lost it replaces what the lost one and
    // its successors claimed.
    private static List<(ulong Revision, ulong Written)> Surviving(ICommitLog timeline)
    {
        var entries = new List<(ulong Revision, ulong Written)>();
        foreach (var entry in timeline.Read())
        {
            var fields = entry.Split('|');
            if (fields.Length != 2
                || !ulong.TryParse(fields[0], NumberStyles.None, CultureInfo.InvariantCulture, out var revision)
                || !ulong.TryParse(fields[1], NumberStyles.None, CultureInfo.InvariantCulture, out var written))
                throw new InvalidDataException("Malformed timeline record.");
            while (entries.Count > 0 && entries[^1].Revision >= revision) entries.RemoveAt(entries.Count - 1);
            entries.Add((revision, written));
        }
        return entries;
    }
}
