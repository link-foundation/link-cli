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
        _replaying = true;
        try
        {
            for (var index = replayFrom; index < _log.Count; index++)
            {
                var transition = _log[index];
                if (!_committed.Contains(transition.TransactionId)) continue;
                TryApplyTransition(transition);
            }

            // Auto-rollback transitions written but never committed and never
            // rolled back: this is the crash-mid-transaction case (R10).
            foreach (var transition in _log.OrderByDescending(t => t.Sequence))
            {
                if (_committed.Contains(transition.TransactionId)) continue;
                if (_rolledBack.Contains(transition.TransactionId)) continue;
                TryRevertTransition(transition);
            }

            // Mark recovered-but-incomplete transactions as rolled back so
            // we don't try to revert them on the next open.
            var pendingTxIds = _log
              .Where(t => !_committed.Contains(t.TransactionId) && !_rolledBack.Contains(t.TransactionId))
              .Select(t => t.TransactionId)
              .Distinct()
              .ToList();
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
