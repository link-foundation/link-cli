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

    // Caller holds Gate continuously through construction and publication.
    private IReadOnlyList<string> CheckpointLocked()
    {
        IEnumerable<(EngineSnapshot<T> Snapshot, BigInteger Next)> snapshots = _history.Count == 0 ? new[] { (Current, CommittedNext) } : _history.Values;
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
