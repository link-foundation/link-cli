using System.Numerics;

namespace Foundation.Data.Doublets.Cli.Engine;

public sealed partial class LinksEngine<T>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    /// <summary>Export retained committed snapshots without rewriting the live
    /// log or including pending reservations. The caller owns atomic publication
    /// and synchronization. Older readers reject the engine3 first record.</summary>
    public IReadOnlyList<string> ExportCheckpoint()
    {
        lock (Gate)
        {
            CheckReady();
            return CheckpointLocked();
        }
    }

    /// <summary>Export the retained snapshots up to one retained revision, as a
    /// journal whose reopened head is that revision.</summary>
    public IReadOnlyList<string> ExportCheckpoint(ulong revision)
    {
        lock (Gate)
        {
            CheckReady();
            return RetainedLocked(revision);
        }
    }

    /// <summary>Copy a consistent cut to an empty log while commits continue, and
    /// return the revision it ends at. Null selects the current revision; a retained
    /// earlier revision gives a point-in-time copy. The gate is held only while the
    /// cut is encoded, never during destination I/O, so a slow or failing destination
    /// cannot delay or fence this engine. After a failure the destination may hold a
    /// shorter copy, so discard it. See docs/ENGINE-BACKUP.md.</summary>
    public ulong Backup(ICommitLog destination, ulong? revision = null)
    {
        ArgumentNullException.ThrowIfNull(destination);
        if (destination.Read().Count != 0) throw new InvalidOperationException("Backup destination must be empty.");
        IReadOnlyList<string> records;
        ulong copied;
        lock (Gate)
        {
            CheckReady();
            copied = revision ?? Current.Revision;
            records = RetainedLocked(copied);
        }
        foreach (var record in records) destination.Append(record);
        destination.Flush();
        if (!destination.Read().SequenceEqual(records))
            throw new InvalidDataException("Backup verification failed: destination differs from the copied records.");
        return copied;
    }

    /// <summary>Bring an earlier backup of this engine up to the current revision by
    /// appending only the revisions it lacks, each as the rows that changed, and return
    /// the revision it now ends at. An empty destination gets a full copy. The
    /// destination's last revision must still be retained here and hold the same rows,
    /// otherwise it is refused and left alone: take a full backup. The gate is held only
    /// to select the snapshots. After a failure the destination may end at an earlier
    /// revision, and calling again continues from there. See docs/ENGINE-BACKUP.md.</summary>
    public ulong BackupIncremental(ICommitLog destination)
    {
        ArgumentNullException.ThrowIfNull(destination);
        var existing = destination.Read();
        if (existing.Count == 0) return Backup(destination);
        StateStructure structure;
        lock (Gate)
        {
            CheckReady();
            structure = Options.StateStructure;
        }
        var last = EngineSnapshot<T>.Create(0, new(structure));
        var lastNext = BigInteger.One;
        var first = true;
        foreach (var record in existing)
        {
            (last, lastNext) = first && record.StartsWith("engine3|", StringComparison.Ordinal)
                ? RevisionJournal<T>.DecodeCheckpoint(record, structure)
                : RevisionJournal<T>.DecodeAfter(record, last, lastNext, structure);
            first = false;
        }
        (EngineSnapshot<T> Snapshot, BigInteger Next)[] chain;
        lock (Gate)
        {
            CheckReady();
            chain = _history.Count == 0
                ? last.Revision == Current.Revision ? [(Current, CommittedNext)] : []
                : _history.Values.Where(item => item.Snapshot.Revision >= last.Revision).ToArray();
        }
        if (chain.Length == 0 || chain[0].Snapshot.Revision != last.Revision)
            throw new KeyNotFoundException("Revision is not retained.");
        if (chain[0].Next != lastNext
            || !chain[0].Snapshot.Rows.OrderBy(row => row.Key).SequenceEqual(last.Rows.OrderBy(row => row.Key)))
            throw new InvalidOperationException("Backup destination is not a copy of this engine.");
        var records = chain.Zip(chain.Skip(1), (previous, item) => ChangeRevisionJournal<T>.Encode(
            item.Snapshot, item.Next, previous.Snapshot,
            previous.Snapshot.Rows.Keys.Concat(item.Snapshot.Rows.Keys))).ToArray();
        foreach (var record in records) destination.Append(record);
        destination.Flush();
        if (!destination.Read().SequenceEqual(existing.Concat(records)))
            throw new InvalidDataException("Backup verification failed: destination differs from the copied records.");
        return chain[^1].Snapshot.Revision;
    }

    /// <summary>Copy an archived journal to an empty log, stopping at one revision, and
    /// return the revision the copy ends at. Null copies the whole archive. Opening the
    /// destination gives the engine as it was at that revision, on a new timeline.
    /// Records are copied unchanged, so an incremental chain restores to any revision it
    /// holds, and so does a live journal. Records after the selected revision are not
    /// decoded, so a copy can end just before damage. Everything up to it is decoded
    /// before the first write, so a refused restore leaves the destination unchanged.
    /// After a later failure the destination may hold a shorter copy, so discard it.
    /// See docs/ENGINE-BACKUP.md.</summary>
    public static ulong RestoreBackup(ICommitLog source, ICommitLog destination, ulong? revision = null)
    {
        ArgumentNullException.ThrowIfNull(source);
        ArgumentNullException.ThrowIfNull(destination);
        if (destination.Read().Count != 0) throw new InvalidOperationException("Restore destination must be empty.");
        var records = source.Read();
        const StateStructure structure = StateStructure.Flat;
        var last = EngineSnapshot<T>.Create(0, new(structure));
        var lastNext = BigInteger.One;
        // A journal without a checkpoint starts from the empty revision 0.
        var reached = revision == 0 && !(records.Count > 0 && records[0].StartsWith("engine3|", StringComparison.Ordinal));
        var kept = 0;
        foreach (var record in records)
        {
            if (reached) break;
            (last, lastNext) = kept == 0 && record.StartsWith("engine3|", StringComparison.Ordinal)
                ? RevisionJournal<T>.DecodeCheckpoint(record, structure)
                : RevisionJournal<T>.DecodeAfter(record, last, lastNext, structure);
            kept++;
            reached = last.Revision == revision;
        }
        if (revision is not null && !reached) throw new KeyNotFoundException("Revision is not in the backup.");
        var copied = records.Take(kept).ToArray();
        foreach (var record in copied) destination.Append(record);
        destination.Flush();
        if (!destination.Read().SequenceEqual(copied))
            throw new InvalidDataException("Restore verification failed: destination differs from the copied records.");
        return last.Revision;
    }

    private IReadOnlyList<string> RetainedLocked(ulong revision)
    {
        if (revision != Current.Revision && !_history.ContainsKey(revision))
            throw new KeyNotFoundException("Revision is not retained.");
        return CheckpointLocked(revision);
    }

    // Caller holds Gate continuously through construction and publication.
    private IReadOnlyList<string> CheckpointLocked() => CheckpointLocked(Current.Revision);

    private IReadOnlyList<string> CheckpointLocked(ulong revision)
    {
        IEnumerable<(EngineSnapshot<T> Snapshot, BigInteger Next)> snapshots = _history.Count == 0
            ? new[] { (Current, CommittedNext) }
            : _history.Values.Where(item => item.Snapshot.Revision <= revision);
        return snapshots.Select((item, index) => RevisionJournal<T>.EncodeFormat(item.Snapshot,
            item.Next,
            index == 0 ? "engine3" : "engine1")).ToArray();
    }
    /// <summary>Publish retained committed snapshots through an optional atomic
    /// replacement log. An unknown outcome fences this owner until reopen.</summary>
    // The revision the last compaction published, for CompactJournalAfter.
    private ulong _compactedRevision;

    public bool CompactJournal() => CompactJournalWhen(_ => true);

    /// <summary>Compact once at least <paramref name="commits"/> revisions have
    /// landed since the last compaction by this owner, and otherwise do nothing and
    /// return false. A log that cannot be replaced is refused either way. The first
    /// call after opening counts from revision 0, which also compacts a journal
    /// left long by an earlier owner.</summary>
    public bool CompactJournalAfter(ulong commits)
    {
        ArgumentOutOfRangeException.ThrowIfZero(commits);
        return CompactJournalWhen(landed => landed >= commits);
    }

    private bool CompactJournalWhen(Func<ulong, bool> due)
    {
        lock (Gate)
        {
            CheckReady();
            if (_log is not IReplaceableCommitLog { SupportsReplacement: true } replacement)
                throw new NotSupportedException("Selected log does not support journal replacement.");
            var revision = Current.Revision;
            if (!due(revision - _compactedRevision)) return false;
            var records = CheckpointLocked();
            try
            {
                var changed = replacement.Replace(records);
                _compactedRevision = revision;
                return changed;
            }
            catch { Poisoned = true; throw; }
        }
    }
}
