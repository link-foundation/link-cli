using System.Numerics;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli;

public partial class TransactionsDecorator<TLinkAddress> where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    private void RecordTransitions(Transaction transaction, TransitionKind kind,
        IEnumerable<(Link<TLinkAddress>? Before, Link<TLinkAddress>? After)> observed)
    {
        lock (_lock)
        {
            var states = observed.ToArray();
            var exhausted = states.LongLength > long.MaxValue - _sequenceCounter;
            Trace($"Preparing {states.Length} transitions at seq={_sequenceCounter}, exhausted={exhausted}.");
            var prepared = new List<Transition<TLinkAddress>>(states.Length);
            // Keep the complete undo set before assigning or writing any batch.
            foreach (var state in states)
            {
                var transition = new Transition<TLinkAddress>(
                    transaction.Id, exhausted ? 0 : ++_sequenceCounter, DateTimeOffset.UtcNow,
                    kind, state.Before ?? default, state.After ?? default);
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
                    Trace($"Recorded {kind} seq={transition.Sequence} tx={transaction.Id:N}: ({transition.Before.Index},{transition.Before.Source},{transition.Before.Target}) -> ({transition.After.Index},{transition.After.Source},{transition.After.Target}).");
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
