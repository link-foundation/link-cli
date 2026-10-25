using System.Numerics;

namespace Foundation.Data.Doublets.Cli;

public partial class TransactionsDecorator<TLinkAddress> where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    private TLinkAddress WriteNameLocked(TLinkAddress index, string? name)
    {
        EnsureWriteAllowedLocked();
        // A row callback may write a name through this same owner. Preserve
        // the triggering row's place before this independent name record.
        _rowNameObserver.Value?.Invoke(null);
        var before = _inner.CaptureNames();
        var after = new Dictionary<TLinkAddress, string>(before);
        if (name is null) after.Remove(index);
        else
        {
            foreach (var previous in after.Where(pair => _inner.ReassignsNames && pair.Key != index && pair.Value == name).Select(pair => pair.Key).ToArray())
                after.Remove(previous);
            after[index] = name;
        }
        var names = before.Keys.Concat(after.Keys).Distinct().Order().Select(id =>
            new NameChange<TLinkAddress>(id, before.GetValueOrDefault(id), after.GetValueOrDefault(id)))
            .Where(change => change.Before != change.After).ToArray();
        if (names.Length == 0) return SetInnerName(index, name);
        if (_sequenceCounter == long.MaxValue)
            throw new InvalidOperationException("Transaction sequence space is exhausted.");
        var owns = _current is null;
        var transaction = _current ??= new Transaction(this, autoCommit: true);
        try
        {
            RecordRecords(transaction, [new(new(transaction.Id, 0, default, TransitionKind.Update, default, default), true, names)]);
            // Persist undo before a memory-mapped name sidecar can change.
            _flushLog();
        }
        catch
        {
            transaction.MarkRollbackOnly();
            if (owns) transaction.Rollback();
            throw;
        }
        TLinkAddress result;
        try
        {
            result = SetInnerName(index, name);
            _rowNameObserver.Value?.Invoke(after);
        }
        catch
        {
            transaction.MarkRollbackOnly();
            if (owns) transaction.Rollback();
            throw;
        }
        // An uncertain commit is completed through its retained owner.
        if (owns) transaction.Commit();
        return result;
    }

    private TLinkAddress SetInnerName(TLinkAddress index, string? name)
    {
        if (name is not null) return _inner.SetName(index, name);
        _inner.RemoveName(index); return TLinkAddress.Zero;
    }

    private void ReplayNames(JournalRecord<TLinkAddress> record, bool undo)
    {
        // Clear all changed owners before restoring a moved unique name.
        foreach (var change in record.Names) _inner.RemoveName(change.Index);
        foreach (var change in record.Names)
        {
            var name = undo ? change.Before : change.After;
            if (name is not null) _inner.SetName(change.Index, name);
        }
    }

    public void ApplyRecord(JournalRecord<TLinkAddress> record) => ReplayRecord(record, false);
    public void RevertRecord(JournalRecord<TLinkAddress> record) => ReplayRecord(record, true);

    private void ReplayRecord(JournalRecord<TLinkAddress> record, bool undo)
    {
        ArgumentNullException.ThrowIfNull(record);
        lock (_lock)
        {
            EnsureReplayAllowedLocked();
            if (!record.IsValid()) throw new InvalidDataException("Invalid name state in transaction record.");
            _replaying = true;
            try { if (undo) TryRevertRecord(record); else TryApplyRecord(record); }
            finally { _replaying = false; }
        }
    }

    private void TryApplyRecord(JournalRecord<TLinkAddress> record)
    {
        if (!record.NameOnly) TryApplyTransition(record.Transition);
        ReplayNames(record, false);
    }

    private void TryRevertRecord(JournalRecord<TLinkAddress> record)
    {
        if (!record.NameOnly) TryRevertTransition(record.Transition);
        ReplayNames(record, true);
    }

    private void MarkApplied(JournalRecord<TLinkAddress> record) => MarkApplied(record.Transition);
    private void CheckpointTransition(JournalRecord<TLinkAddress> record) => CheckpointTransition(record.Transition);
}
