using System.Globalization;
using System.Numerics;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli;

public partial class TransactionsDecorator<TLinkAddress> where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    // Recovery --------------------------------------------------------------

    public virtual void Recover()
    {
        lock (_lock)
        {
            if (_current is not null)
            {
                throw new InvalidOperationException("Cannot recover while a transaction is open.");
            }
            _log.Clear();
            _committed.Clear();
            _rolledBack.Clear();
            _applied.Clear();
            _sequenceCounter = 0;
            _appliedSequence = 0;

            var any = _logStore.Constants.Any;
            var anyLink = new Link<TLinkAddress>(any, any, any);
            foreach (var raw in _logStore.All(anyLink))
            {
                var link = new Link<TLinkAddress>(raw);
                var name = _logStore.GetName(link.Index);
                if (string.IsNullOrEmpty(name)) continue;

                if (name.StartsWith(TransitionNamePrefix, StringComparison.Ordinal))
                {
                    var payload = name.Substring(TransitionNamePrefix.Length);
                    if (Transition<TLinkAddress>.TryParse(payload, out var transition))
                    {
                        InsertOrdered(_log, transition);
                        if (transition.Sequence > _sequenceCounter)
                        {
                            _sequenceCounter = transition.Sequence;
                        }
                    }
                    else throw new InvalidDataException("Malformed transaction record in journal.");
                }
                else if (name.StartsWith(CommitMarkerPrefix, StringComparison.Ordinal))
                {
                    if (Guid.TryParseExact(name.Substring(CommitMarkerPrefix.Length), "N", out var txId))
                    {
                        _committed.Add(txId);
                    }
                    else throw new InvalidDataException("Malformed commit marker in journal.");
                }
                else if (name.StartsWith(RollbackMarkerPrefix, StringComparison.Ordinal))
                {
                    if (Guid.TryParseExact(name.Substring(RollbackMarkerPrefix.Length), "N", out var txId))
                    {
                        _rolledBack.Add(txId);
                    }
                    else throw new InvalidDataException("Malformed rollback marker in journal.");
                }
                else if (name.StartsWith(AppliedMarkerPrefix, StringComparison.Ordinal))
                {
                    var rest = name.Substring(AppliedMarkerPrefix.Length);
                    if (long.TryParse(rest, NumberStyles.Integer, CultureInfo.InvariantCulture, out var seq) && seq > 0)
                    {
                        _applied.Add(seq);
                        if (seq > _appliedSequence) _appliedSequence = seq;
                    }
                    else throw new InvalidDataException("Malformed applied marker in journal.");
                }
            }

            _replaying = true;
            try
            {
                // Re-apply committed transitions whose side-effects were lost
                // (e.g. async crash before checkpoint). Only those not yet
                // recorded as applied are touched.
                foreach (var transition in _log)
                {
                    if (!_committed.Contains(transition.TransactionId)) continue;
                    if (_applied.Contains(transition.Sequence)) continue;
                    TryApplyTransition(transition, recordApplied: true);
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

}
