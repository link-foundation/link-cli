using System.Numerics;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli;

public partial class TransactionsDecorator<TLinkAddress> where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    private void RecordTransitions(Transaction transaction,
        IEnumerable<(Link<TLinkAddress>? Before, Link<TLinkAddress>? After)> observed,
        IReadOnlyDictionary<TLinkAddress, string> namesBefore,
        IReadOnlyDictionary<TLinkAddress, string>? namesAfter)
    {
        var records = observed.Select(state =>
        {
            var before = state.Before ?? default;
            var after = state.After ?? default;
            var index = after.Index == TLinkAddress.Zero ? before.Index : after.Index;
            var oldName = namesBefore.GetValueOrDefault(index);
            // A missing after map is pending undo after a failed capture.
            var newName = namesAfter is null ? after.Index == TLinkAddress.Zero ? null : oldName
                : namesAfter.GetValueOrDefault(index);
            var names = oldName is not null || newName is not null
                ? new[] { new NameChange<TLinkAddress>(index, oldName, newName) }
                : Array.Empty<NameChange<TLinkAddress>>();
            var actualKind = before.Index == TLinkAddress.Zero ? TransitionKind.Create
                : after.Index == TLinkAddress.Zero ? TransitionKind.Delete : TransitionKind.Update;
            return new JournalRecord<TLinkAddress>(new(transaction.Id, 0, default, actualKind, before, after), names: names);
        });
        RecordRecords(transaction, records);
    }

    private void RecordRecords(Transaction transaction, IEnumerable<JournalRecord<TLinkAddress>> records)
    {
        lock (_lock)
        {
            var states = records.ToArray();
            var exhausted = states.LongLength > long.MaxValue - _sequenceCounter;
            Trace($"Preparing {states.Length} transitions at seq={_sequenceCounter}, exhausted={exhausted}.");
            var prepared = new List<JournalRecord<TLinkAddress>>(states.Length);
            // Keep the complete undo set before assigning or writing any batch.
            foreach (var state in states)
            {
                var row = state.Transition with
                {
                    TransactionId = transaction.Id,
                    Sequence = exhausted ? 0 : ++_sequenceCounter,
                    Timestamp = DateTimeOffset.UtcNow
                };
                var transition = new JournalRecord<TLinkAddress>(row, state.NameOnly, state.Names);
                transaction.AddTransition(transition);
                if (!exhausted) _log.Add(transition);
                prepared.Add(transition);
            }
            if (exhausted)
            {
                // These unnumbered records are pending undo only. Never persist
                // them or expose them as numbered transaction history.
                transaction.MarkRollbackOnly();
                throw new InvalidOperationException("Transaction sequence space is exhausted.");
            }
            try
            {
                foreach (var transition in prepared)
                {
                    WriteTransitionToLog(transition);
                    Trace($"Recorded {transition.Kind} seq={transition.Sequence} tx={transaction.Id:N}: ({transition.Before.Index},{transition.Before.Source},{transition.Before.Target}) -> ({transition.After.Index},{transition.After.Source},{transition.After.Target}).");
                }
            }
            catch
            {
                transaction.MarkRollbackOnly();
                throw;
            }
        }
    }
}
