using System.Globalization;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Optional single-span physical compression of complete revisions.
/// Reopen through this decorator; one writer owns the inner log and its fencing.
/// Logical changes pass through unchanged. Resources remain caller-owned.</summary>
public sealed class DeltaCommitLog : IReplaceableCommitLog
{
    private readonly ICommitLog _inner;
    private readonly uint _interval;
    private Frame? _previous;
    private ulong _streak;
    private bool _ready;
    private bool _poisoned;
    private enum Kind { Snapshot, Changes, Checkpoint }
    private sealed record Frame(ulong Revision, string Next, string Rows, string Hash, Kind Format)
    {
        public bool Complete => Format != Kind.Changes;
    }

    /// <param name="inner">Caller-owned, exclusively fenced physical log.</param>
    /// <param name="checkpointInterval">At most interval-1 consecutive deltas; 1 writes only full records.</param>
    public DeltaCommitLog(ICommitLog inner, uint checkpointInterval = 128)
    {
        ArgumentNullException.ThrowIfNull(inner);
        if (checkpointInterval == 0) throw new ArgumentOutOfRangeException(nameof(checkpointInterval));
        _inner = inner;
        _interval = checkpointInterval;
    }
    public bool SupportsDurableFlush => _inner.SupportsDurableFlush;
    public bool SupportsReplacement => _inner is IReplaceableCommitLog { SupportsReplacement: true };
    public bool Replace(IReadOnlyList<string> records)
    {
        Check();
        if (_inner is not IReplaceableCommitLog { SupportsReplacement: true } replacement)
            throw new NotSupportedException("Selected log does not support journal replacement.");
        ArgumentNullException.ThrowIfNull(records);
        var memory = new MemoryCommitLog();
        var prepared = new DeltaCommitLog(memory, _interval);
        prepared.Read();
        foreach (string record in records.ToArray()) prepared.Append(record);
        try
        {
            bool changed = replacement.Replace(memory.Read());
            _previous = prepared._previous; _streak = prepared._streak; _ready = true;
            return changed;
        }
        catch { _poisoned = true; throw; }
    }
    private void Check()
    {
        if (_poisoned) throw new IOException("Indeterminate delta journal outcome; reopen the decorator.");
    }
    private static UInt128 Decimal(string text, UInt128 max)
    {
        if (text.Length == 0 || text.Length > 39 || (text.Length > 1 && text[0] == '0') ||
            !text.All(c => c is >= '0' and <= '9') ||
            !UInt128.TryParse(text, NumberStyles.None, CultureInfo.InvariantCulture, out var value) || value > max)
            throw new InvalidDataException("Invalid delta journal decimal.");
        return value;
    }
    private static string Checksum(string text) => RevisionJournal<UInt128>.Checksum(text).ToString("x16", CultureInfo.InvariantCulture);
    private static string Checked(string text) => text + "|" + Checksum(text);
    private static string[] Verified(string record)
    {
        if (record is null || !record.All(c => c is >= ' ' and <= '~'))
            throw new InvalidDataException("Delta journal requires printable ASCII records.");
        int separator = record.LastIndexOf('|');
        if (separator < 0 || record[(separator + 1)..] != Checksum(record[..separator]))
            throw new InvalidDataException("Delta journal checksum mismatch.");
        return record.Split('|');
    }
    private static Frame Full(string record)
    {
        var fields = Verified(record);
        Kind kind = fields[0] switch
        {
            "engine1" when fields.Length == 5 => Kind.Snapshot,
            "engine2" when fields.Length == 6 => Kind.Changes,
            "engine3" when fields.Length == 5 => Kind.Checkpoint,
            _ => throw new InvalidDataException("Expected an engine revision.")
        };
        int offset = kind == Kind.Changes ? 1 : 0;
        ulong revision = (ulong)Decimal(fields[1], ulong.MaxValue);
        UInt128 next = Decimal(fields[2 + offset], UInt128.MaxValue);
        if (next == 0 || (revision == 0 && kind != Kind.Checkpoint))
            throw new InvalidDataException("Invalid delta journal revision or allocator.");
        if (revision == 0 && (next != 1 || fields[3].Length != 0))
            throw new InvalidDataException("Invalid revision-zero checkpoint.");
        if (kind == Kind.Changes && Decimal(fields[2], ulong.MaxValue) != revision - 1)
            throw new InvalidDataException("Invalid logical change base.");
        return new(revision, fields[2 + offset], fields[3 + offset], fields[4 + offset], kind);
    }
    private static void Contiguous(Frame? before, Frame after)
    {
        if (after.Format == Kind.Checkpoint)
        {
            if (before is not null) throw new InvalidDataException("Checkpoint must be the first delta journal record.");
            return;
        }
        UInt128 expected = before is null ? 1 : (UInt128)before.Revision + 1;
        if (expected != after.Revision) throw new InvalidDataException("Delta journal revisions are not contiguous.");
    }
    private static string Expand(Frame? before, string record)
    {
        if (record is null) throw new InvalidDataException("Delta journal requires string records.");
        if (!record.StartsWith("delta1|", StringComparison.Ordinal)) return record;
        var fields = Verified(record);
        if (fields.Length != 9) throw new InvalidDataException("Invalid delta journal envelope.");
        var previous = before ?? throw new InvalidDataException("Delta journal base is missing.");
        if (!previous.Complete) throw new InvalidDataException("Delta requires a complete snapshot base.");
        if (fields[1] != previous.Hash) throw new InvalidDataException("Delta journal base checksum mismatch.");
        var prefix = Decimal(fields[4], uint.MaxValue);
        var suffix = Decimal(fields[5], uint.MaxValue);
        if (prefix > (uint)previous.Rows.Length || suffix > (uint)previous.Rows.Length - prefix)
            throw new InvalidDataException("Delta journal span outside base.");
        return $"engine1|{fields[2]}|{fields[3]}|{previous.Rows[..(int)prefix]}{fields[6]}{previous.Rows[(previous.Rows.Length - (int)suffix)..]}|{fields[7]}";
    }
    private static string Compress(Frame a, Frame b, string full)
    {
        int prefix = 0, suffix = 0;
        int length = Math.Min(a.Rows.Length, b.Rows.Length);
        while (prefix < length && a.Rows[prefix] == b.Rows[prefix]) prefix++;
        while (suffix < length - prefix && a.Rows[a.Rows.Length - suffix - 1] == b.Rows[b.Rows.Length - suffix - 1]) suffix++;
        var text = string.Join("|", "delta1", a.Hash, b.Revision.ToString(CultureInfo.InvariantCulture), b.Next,
            prefix.ToString(CultureInfo.InvariantCulture), suffix.ToString(CultureInfo.InvariantCulture), b.Rows[prefix..(b.Rows.Length - suffix)], b.Hash);
        var delta = Checked(text);
        return delta.Length < full.Length ? delta : full;
    }
    public IReadOnlyList<string> Read()
    {
        Check();
        _ready = false;
        var logical = new List<string>();
        Frame? previous = null;
        ulong streak = 0;
        foreach (var record in _inner.Read() ?? throw new InvalidDataException("Delta journal requires a record list."))
        {
            var full = Expand(previous, record);
            var parsed = Full(full);
            Contiguous(previous, parsed);
            previous = parsed;
            logical.Add(full);
            streak = record.StartsWith("delta1|", StringComparison.Ordinal) ? streak + 1 : 0;
        }
        _previous = previous;
        _streak = streak; _ready = true;
        return logical.ToArray();
    }
    public void Append(string record)
    {
        Check();
        if (!_ready) Read();
        var parsed = Full(record);
        Contiguous(_previous, parsed);
        string physical = _previous is { Complete: true } && parsed.Complete && _streak + 1 < _interval ? Compress(_previous, parsed, record) : record;
        try { _inner.Append(physical); }
        catch { _poisoned = true; throw; }
        _streak = physical.StartsWith("delta1|", StringComparison.Ordinal) ? _streak + 1 : 0;
        _previous = parsed;
    }
    public void Flush()
    {
        Check();
        try { _inner.Flush(); }
        catch { _poisoned = true; throw; }
    }
}
