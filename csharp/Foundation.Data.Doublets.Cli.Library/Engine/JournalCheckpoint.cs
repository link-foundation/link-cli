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
