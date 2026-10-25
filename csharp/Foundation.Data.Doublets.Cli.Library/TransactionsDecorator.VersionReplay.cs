using System.Numerics;

namespace Foundation.Data.Doublets.Cli;

public partial class TransactionsDecorator<TLinkAddress> where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    private bool _versionReplay;
    private Action? _versionReplayRecovery;

    internal void RetainVersionReplayRecovery(Action recovery)
    {
        lock (_lock)
        {
            RequireVersionReplayLocked();
            _versionReplayRecovery = recovery;
        }
    }

    internal void ResumeVersionReplayRecovery()
    {
        // Only failed constructors register callbacks: their owner cannot be
        // reached by another caller, so this lock cannot invert a live facade.
        lock (_lock) { _versionReplayRecovery?.Invoke(); }
    }

    private void EnsureNoVersionReplayLocked()
    {
        if (_versionReplay)
            throw new InvalidOperationException("Complete version movement before accessing transaction writes or persistence.");
    }

    internal void BeginVersionReplay()
    {
        lock (_lock)
        {
            EnsureRecoveryCompleteLocked();
            if (_current is not null)
                throw new InvalidOperationException("A transaction still owns the data store.");
            _versionReplay = true;
            // Inline writes already contain every committed effect. Establish
            // their applied markers before movement changes those rows. Queued
            // checkpoints subsequently observe these markers and need no redo.
            _flushData();
            foreach (var transition in _log)
                if (_committed.Contains(transition.TransactionId)) MarkApplied(transition);
            _flushLog();
        }
    }

    internal void ReplayVersionTransition(Transition<TLinkAddress> transition, bool undo)
    {
        lock (_lock)
        {
            RequireVersionReplayLocked();
            _replaying = true;
            try
            {
                if (undo) TryRevertTransition(transition);
                else TryApplyTransition(transition);
            }
            finally { _replaying = false; }
        }
    }

    internal void FlushVersionReplay()
    {
        lock (_lock)
        {
            RequireVersionReplayLocked();
            _flushData();
            _flushLog();
        }
    }

    internal void FinishVersionReplay()
    {
        lock (_lock) { _versionReplay = false; _versionReplayRecovery = null; }
    }

    private void RequireVersionReplayLocked()
    {
        if (!_versionReplay) throw new InvalidOperationException("Version movement does not own replay.");
    }
}
