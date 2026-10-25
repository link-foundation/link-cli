using System.Numerics;

namespace Foundation.Data.Doublets.Cli;

public partial class TransactionsDecorator<TLinkAddress> where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    // Recovery --------------------------------------------------------------

    private bool _recoveryPending;

    private void EnsureRecoveryCompleteLocked()
    {
        if (_recoveryPending)
            throw new InvalidOperationException("Retry recovery before further transaction writes or replay.");
    }

    public virtual void Recover()
    {
        lock (_lock)
        {
            if (_current is not null || _versionReplay)
                throw new InvalidOperationException("Cannot recover while a transaction is open.");
            _recoveryPending = true;
            try
            {
                RecoverJournal();
                _recoveryPending = false;
            }
            catch (Exception error)
            {
                if (_trace) Trace($"Recovery failed; admission retained: {error.Message}");
                throw;
            }
        }
    }

    private void RecoverJournal()
    {
        var journal = ReadRecoveryJournal();
        _log.Clear();
        _log.AddRange(journal.Transitions);
        _committed.Clear();
        _committed.UnionWith(journal.Committed);
        _rolledBack.Clear();
        _rolledBack.UnionWith(journal.RolledBack);
        _applied.Clear();
        _applied.UnionWith(journal.Applied);
        _sequenceCounter = journal.Sequence;
        _appliedSequence = journal.AppliedSequence;

        // An older async checkpoint can fail before a later commit has
        // already been applied. Redo the whole suffix in sequence so the
        // older state cannot overwrite that later durable result.
        var replayFrom = _log.FindIndex(t => _committed.Contains(t.TransactionId) && !_applied.Contains(t.Sequence));
        if (replayFrom < 0) replayFrom = _log.Count;
        var images = journal.Images.Where(image => !_committed.Contains(image.Owner) && !_rolledBack.Contains(image.Owner)).ToArray();
        if (images.Length != 0)
        {
            var start = _log.FindIndex(record => record.Sequence > images.Min(image => image.Base));
            if (start < 0) start = _log.Count;
            replayFrom = Math.Min(replayFrom, start);
        }
        var pendingTxIds = _log.Where(record => !_committed.Contains(record.TransactionId) && !_rolledBack.Contains(record.TransactionId))
            .Select(record => record.TransactionId).Concat(images.Select(image => image.Owner)).Distinct().ToList();
        _replaying = true;
        try
        {
            if (images.Length != 0)
            {
                var imaged = images.Select(image => image.Owner).ToHashSet();
                var records = _log.AsEnumerable().Reverse().Where(record => !_committed.Contains(record.TransactionId)
                    && !_rolledBack.Contains(record.TransactionId) && !imaged.Contains(record.TransactionId)).ToArray();
                var next = 0;
                // Equal-base pending owners are rejected before any mutation.
                foreach (var image in images.OrderBy(image => image.Base).Reverse())
                {
                    while (next < records.Length && records[next].Sequence > image.Base) TryRevertRecord(records[next++]);
                    RestoreImage(image);
                }
                while (next < records.Length) TryRevertRecord(records[next++]);
            }
            for (var index = replayFrom; index < _log.Count; index++)
            {
                var transition = _log[index];
                if (!_committed.Contains(transition.TransactionId)) continue;
                TryApplyRecord(transition);
            }

            // Auto-rollback transitions written but never committed and never
            // rolled back: this is the crash-mid-transaction case (R10).
            foreach (var transition in _log.OrderByDescending(t => t.Sequence))
            {
                if (images.Length != 0) break;
                if (_committed.Contains(transition.TransactionId)) continue;
                if (_rolledBack.Contains(transition.TransactionId)) continue;
                TryRevertRecord(transition);
            }

            // Mark recovered-but-incomplete transactions as rolled back so
            // we don't try to revert them on the next open.
            _flushData();
            // Keep the first gap as a restart anchor until both the later
            // redo and pending undo are durable. Marking it sooner could
            // make an interrupted recovery skip the newer committed state.
            for (var index = replayFrom; index < _log.Count; index++)
            {
                var transition = _log[index];
                if (_committed.Contains(transition.TransactionId)) MarkApplied(transition);
            }
            foreach (var txId in pendingTxIds)
            {
                _rolledBack.Add(txId);
                WriteMarker(RollbackMarkerPrefix + txId.ToString("N"));
            }
        }
        finally
        {
            _replaying = false;
        }
        _flushLog();
    }
}
