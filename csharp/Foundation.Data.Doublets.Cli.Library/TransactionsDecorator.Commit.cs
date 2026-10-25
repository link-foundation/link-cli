using System.Numerics;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli;

public partial class TransactionsDecorator<TLinkAddress> where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    internal void OnCommit(Transaction transaction, bool forceAsync)
    {
        lock (_lock)
        {
            if (transaction.IsRolledBack) return;
            // A durable decision can still own an incomplete synchronous
            // checkpoint. Successful/queued transactions have released it.
            var retry = transaction.IsCommitAttempted;
            if (transaction.IsCommitted && _current != transaction) return;
            if (!transaction.IsCommitted)
            {
                if (transaction.IsRollbackOnly)
                {
                    throw new InvalidOperationException("Cannot commit a rollback-only transaction; rollback is required.");
                }
                // Append/flush may persist and then throw. Fence writes before
                // either operation and resolve this decision through cleanup.
                transaction.MarkCommitAttempted();
                var marker = CommitMarkerPrefix + transaction.Id.ToString("N");
                if (!retry || !ContainsCommitMarker(marker)) WriteMarker(marker);
                _flushLog();
                _committed.Add(transaction.Id);
                transaction.MarkCommitted();
            }

            // Retrying an uncertain/known decision finishes synchronously, even through
            // CommitAsync, so cleanup does not silently detach failed work.
            var runAsync = !retry && (forceAsync || _commitMode == CommitMode.Async);
            Trace($"Committed tx {transaction.Id:N} (mode={(runAsync ? "async" : "sync")}, transitions={transaction.Records.Count}).");
            if (runAsync)
            {
                var transitions = transaction.Records.ToArray();
                if (!_asyncQueue.Writer.TryWrite(() => ApplyTransitions(transitions)))
                    throw new InvalidOperationException("Background checkpoint queue is closed.");
            }
            else
            {
                _flushData();
                foreach (var transition in transaction.Records) MarkApplied(transition);
                _flushLog();
                EnforceRetentionLocked();
            }
            // Failed persistence or queue admission keeps the owner available
            // to Commit, Rollback or Dispose without undoing the decision.
            if (_current == transaction) _current = null;
        }
    }

    internal void OnRollback(Transaction transaction)
    {
        lock (_lock)
        {
            if (transaction.IsCommitAttempted)
            {
                OnCommit(transaction, forceAsync: false);
                return;
            }
            if (transaction.IsRolledBack) return;
            // A failed undo or persistence step must preserve the abort decision.
            transaction.MarkRollbackOnly();
            _replaying = true;
            try
            {
                if (transaction.Image is not null) RestoreImage(transaction.Image);
                else foreach (var transition in transaction.Records.AsEnumerable().Reverse())
                    {
                        TryRevertRecord(transition);
                    }
            }
            finally
            {
                _replaying = false;
            }
            _flushData();
            WriteMarker(RollbackMarkerPrefix + transaction.Id.ToString("N"));
            _flushLog();
            transaction.MarkRolledBack();
            _rolledBack.Add(transaction.Id);
            if (_current == transaction) _current = null;
            Trace($"Rolled back tx {transaction.Id:N} ({transaction.Records.Count} transitions).");
            EnforceRetentionLocked();
        }
    }

    private void EnsureWriteAllowedLocked()
    {
        EnsureNoVersionReplayLocked();
        EnsureRecoveryCompleteLocked();
        if (_current?.IsCommitAttempted == true)
        {
            throw new InvalidOperationException("Transaction commit requires retry before further writes; its outcome may be unknown.");
        }
        if (_current?.IsRollbackOnly == true)
        {
            throw new InvalidOperationException("Transaction requires rollback before further writes.");
        }
    }

    private bool ContainsCommitMarker(string marker)
    {
        // Only uncertain retries scan; ordinary commits and known-checkpoint
        // retries keep their existing cost and never append another decision.
        var any = _logStore.Constants.Any;
        foreach (var row in _logStore.All(new Link<TLinkAddress>(any, any, any)))
            if (row is not null && string.Equals(_logStore.GetName(row[0]), marker, StringComparison.Ordinal)) return true;
        return false;
    }
}
