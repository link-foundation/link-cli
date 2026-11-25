using System.Globalization;
using System.Numerics;

namespace Foundation.Data.Doublets.Cli;

/// <summary>
/// Bounds the durable journal by the retention policy (issue #195):
/// rust/src/transactions/compaction.rs.
/// </summary>
/// <remarks>
/// <para>
/// Retention trims the in-memory history after each commit or rollback.
/// Without compaction the journal on disk kept every entry ever written, so
/// each process start re-read the complete history and every write grew a
/// file that no policy could shrink. When the log supports it, the journal is
/// rewritten to the entries that still matter once it has grown to about
/// twice that size, so the rewrite is amortized over many appends.
/// </para>
/// <para>
/// A compacted journal keeps, in order: a <c>__transactions:sequence:&lt;n&gt;</c>
/// watermark, so sequence numbers (which version-control tags and branches
/// reference) are never reused; an applied marker for the highest applied
/// sequence; every retained record, unchanged; the commit or rollback decision
/// of every retained record's owner and the newest decision, which
/// version-control publication recovery reads; and the applied markers of
/// retained records.
/// </para>
/// <para>
/// Undo images belong to undecided owners only while a transaction is open or
/// recovery is pending; compaction never runs then, so decided images are
/// dropped. A journal with any undecided record is left untouched.
/// </para>
/// </remarks>
public partial class TransactionsDecorator<TLinkAddress> where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    internal const string SequenceMarkerPrefix = "__transactions:sequence:";

    /// <summary>
    /// Entries a journal may grow by beyond twice its live size before it is
    /// compacted, so small retention windows do not rewrite on every commit.
    /// </summary>
    internal const int CompactionSlack = 256;

    private long _journalEntries;
    private string? _lastDecision;

    /// <summary>Number of entries the durable journal currently holds.</summary>
    public long JournalEntryCount { get { lock (_lock) return _journalEntries; } }

    /// <summary>Appends one journal entry, counting it towards the compaction budget.</summary>
    private void AppendJournalEntry(string entry)
    {
        // Count even a failed append: it may have reached the journal.
        _journalEntries++;
        _logStore.AppendLogEntry(entry);
    }

    /// <summary>Rewrites the durable journal to its retained entries when it has outgrown them.</summary>
    private void CompactJournalIfNeededLocked()
    {
        if (_retentionPolicy is LogRetentionPolicy.Infinite
            || !_logStore.SupportsCompaction
            // A synchronous commit enforces retention before it releases
            // its (already decided) transaction.
            || _current is { IsCommitted: false } || _versionReplay || _recoveryPending)
        {
            return;
        }
        // Each retained record needs at most itself, a decision and an
        // applied marker; the estimate avoids building entries every commit.
        var live = (long)_log.Count * 3 + 3;
        if (_journalEntries <= live * 2 + CompactionSlack) return;
        var entries = CompactedJournal();
        if (entries is null) return;
        // Appends are durable before their replacement is published.
        _flushLog();
        var before = _journalEntries;
        _logStore.CompactLogEntries(entries);
        _journalEntries = entries.Count;
        Trace($"Compacted journal from {before} to {entries.Count} entries.");
    }

    /// <summary>The retained journal, or null while any retained record's owner is still undecided.</summary>
    private List<string>? CompactedJournal()
    {
        var decisions = new List<string>();
        var seen = new HashSet<Guid>();
        foreach (var record in _log)
        {
            if (!seen.Add(record.TransactionId)) continue;
            string prefix;
            if (_committed.Contains(record.TransactionId)) prefix = CommitMarkerPrefix;
            else if (_rolledBack.Contains(record.TransactionId)) prefix = RollbackMarkerPrefix;
            else return null;
            decisions.Add(prefix + record.TransactionId.ToString("N"));
        }
        if (_lastDecision is not null && !decisions.Contains(_lastDecision)) decisions.Add(_lastDecision);
        var entries = new List<string>(_log.Count * 2 + decisions.Count + 2);
        if (_sequenceCounter > 0) entries.Add(SequenceMarkerPrefix + _sequenceCounter.ToString(CultureInfo.InvariantCulture));
        var appliedRetained = _log.Exists(record => record.Sequence == _appliedSequence);
        if (_appliedSequence > 0 && !appliedRetained)
            entries.Add(AppliedMarkerPrefix + _appliedSequence.ToString(CultureInfo.InvariantCulture));
        foreach (var record in _log) entries.Add(TransitionNamePrefix + record.Serialize());
        entries.AddRange(decisions);
        foreach (var record in _log)
        {
            if (_applied.Contains(record.Sequence))
                entries.Add(AppliedMarkerPrefix + record.Sequence.ToString(CultureInfo.InvariantCulture));
        }
        return entries;
    }
}
