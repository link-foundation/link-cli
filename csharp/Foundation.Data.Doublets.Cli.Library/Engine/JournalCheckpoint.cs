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
    public bool CompactJournal()
    {
        lock (Gate)
        {
            CheckReady();
            if (_log is not IReplaceableCommitLog { SupportsReplacement: true } replacement)
                throw new NotSupportedException("Selected log does not support journal replacement.");
            var records = CheckpointLocked();
            try { return replacement.Replace(records); }
            catch { Poisoned = true; throw; }
        }
    }
}
