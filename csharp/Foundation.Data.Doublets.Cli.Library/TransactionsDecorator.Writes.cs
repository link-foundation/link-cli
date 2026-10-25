using System.Numerics;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli;

public partial class TransactionsDecorator<TLinkAddress> where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    private TLinkAddress RunWrite(
      Func<WriteHandler<TLinkAddress>, TLinkAddress> innerCall,
      WriteHandler<TLinkAddress>? userHandler)
    {
        Transaction transaction;
        bool ownsTransaction;
        IReadOnlyDictionary<TLinkAddress, string> namesBefore;
        lock (_lock)
        {
            EnsureWriteAllowedLocked();
            if (_replaying) return innerCall(userHandler ?? NullHandler);
            if (_sequenceCounter == long.MaxValue)
                throw new InvalidOperationException("Transaction sequence space is exhausted.");
            namesBefore = _inner.CaptureNames();
            if (_current is null)
            {
                _current = new Transaction(this, autoCommit: true);
                ownsTransaction = true;
            }
            else
            {
                ownsTransaction = false;
            }
            transaction = _current;
        }

        var @continue = _inner.Constants.Continue;
        var observed = new Dictionary<TLinkAddress, (Link<TLinkAddress>? Before, Link<TLinkAddress>? After)>();
        var observedOrder = new List<TLinkAddress>();

        WriteHandler<TLinkAddress> wrapped = (before, after) =>
        {
            var beforeLink = before is null ? default(Link<TLinkAddress>?) : new Link<TLinkAddress>(before);
            var afterLink = after is null ? default(Link<TLinkAddress>?) : new Link<TLinkAddress>(after);
            var key = beforeLink.HasValue ? beforeLink.Value.Index
                    : afterLink.HasValue ? afterLink.Value.Index
                    : TLinkAddress.Zero;
            if (key != TLinkAddress.Zero)
            {
                if (!observed.TryGetValue(key, out var state))
                {
                    observedOrder.Add(key);
                    state = (beforeLink, afterLink);
                }
                else
                {
                    // Null is the original absent state of a newly created
                    // address, not a missing observation to replace.
                    state = (state.Before, afterLink);
                }
                observed[key] = state;
            }
            return userHandler is null ? @continue : userHandler(before, after);
        };

        void RecordObservedChanges()
        {
            if (observedOrder.Count == 0) return;
            var states = observedOrder.Select(key => observed[key]).ToArray();
            // RecordRecords saves every undo before its first append. Drain
            // here so a failed append is never recorded again in the catch.
            observed.Clear();
            observedOrder.Clear();
            IReadOnlyDictionary<TLinkAddress, string> namesAfter;
            try
            {
                namesAfter = _inner.CaptureNames();
            }
            catch
            {
                transaction.MarkRollbackOnly();
                RecordTransitions(transaction, states, namesBefore, null);
                throw;
            }
            var previousNames = namesBefore;
            namesBefore = namesAfter;
            RecordTransitions(transaction, states, previousNames, namesAfter);
        }

        // Drain observed rows only when a callback writes a name. Ordinary
        // callbacks retain coalescing and still all run before append errors.
        var previousNameObserver = _rowNameObserver.Value;
        _rowNameObserver.Value = names =>
        {
            if (names is null) RecordObservedChanges();
            else namesBefore = names;
        };
        TLinkAddress result;
        try
        {
            PrepareRowUndo(transaction);
            result = innerCall(wrapped);
            RecordObservedChanges();
            CompleteImageObservations(transaction);
        }
        catch
        {
            if (RowUndoPolicy == RowUndoPolicy.WriteAheadImage) transaction.MarkRollbackOnly();
            // A callback describes a change already made, even when the
            // storage adapter or user handler subsequently throws.
            try { RecordObservedChanges(); }
            finally
            {
                if (ownsTransaction)
                {
                    if (transaction.Records.Count > 0 || transaction.Image is not null) transaction.Rollback();
                    else
                    {
                        lock (_lock)
                        {
                            if (_current == transaction) _current = null;
                        }
                    }
                }
            }
            throw;
        }
        finally { _rowNameObserver.Value = previousNameObserver; }

        if (ownsTransaction)
        {
            transaction.Commit();
        }

        return result;
    }

}
