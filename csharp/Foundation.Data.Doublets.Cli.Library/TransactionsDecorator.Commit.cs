using System.Numerics;

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
            var retry = transaction.IsCommitted;
            if (retry && _current != transaction) return;
            if (!retry)
            {
                if (transaction.IsRollbackOnly)
                {
                    throw new InvalidOperationException("Cannot commit a rollback-only transaction; rollback is required.");
                }
                WriteMarker(CommitMarkerPrefix + transaction.Id.ToString("N"));
                _flushLog();
                _committed.Add(transaction.Id);
                transaction.MarkCommitted();
            }

            // Retrying a known decision finishes synchronously, even through
            // CommitAsync, so cleanup does not silently detach failed work.
            var runAsync = !retry && (forceAsync || _commitMode == CommitMode.Async);
            Trace($"Committed tx {transaction.Id:N} (mode={(runAsync ? "async" : "sync")}, transitions={transaction.Transitions.Count}).");
            if (runAsync)
            {
                var transitions = transaction.Transitions.ToArray();
                _asyncQueue.Add(() => Task.Run(() => ApplyTransitionsAsync(transitions)));
            }
            else
            {
                _flushData();
                foreach (var transition in transaction.Transitions) MarkApplied(transition);
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
            if (transaction.IsCommitted)
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
                foreach (var transition in transaction.Transitions.AsEnumerable().Reverse())
                {
                    TryRevertTransition(transition);
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
            Trace($"Rolled back tx {transaction.Id:N} ({transaction.Transitions.Count} transitions).");
            EnforceRetentionLocked();
        }
    }

    private void EnsureWriteAllowedLocked()
    {
        if (_current?.IsCommitted == true)
        {
            throw new InvalidOperationException("Committed transaction requires checkpoint retry before further writes.");
        }
        if (_current?.IsRollbackOnly == true)
        {
            throw new InvalidOperationException("Transaction requires rollback before further writes.");
        }
    }
}
