using System.Globalization;
using System.Numerics;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli;

public partial class TransactionsDecorator<TLinkAddress> where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    private sealed record RecoveryJournal(
        List<JournalRecord<TLinkAddress>> Transitions,
        HashSet<Guid> Committed,
        HashSet<Guid> RolledBack,
        HashSet<long> Applied,
        long Sequence,
        long AppliedSequence);

    // Build temporary indexes before touching live history, rows or markers.
    // Compare parsed values so idempotent retries keep legacy lexical forms.
    private RecoveryJournal ReadRecoveryJournal()
    {
        var transitions = new Dictionary<long, JournalRecord<TLinkAddress>>();
        var committed = new HashSet<Guid>();
        var rolledBack = new HashSet<Guid>();
        var applied = new HashSet<long>();
        long sequence = 0, appliedSequence = 0;
        var any = _logStore.Constants.Any;
        foreach (var raw in _logStore.All(new Link<TLinkAddress>(any, any, any)))
        {
            var name = _logStore.GetName(new Link<TLinkAddress>(raw).Index);
            if (string.IsNullOrEmpty(name)) continue;
            if (name.StartsWith(TransitionNamePrefix, StringComparison.Ordinal))
            {
                if (!JournalRecord<TLinkAddress>.TryParse(name.Substring(TransitionNamePrefix.Length), out var transition))
                    throw new InvalidDataException("Malformed transaction record in journal.");
                if (transition.TransactionId == Guid.Empty || transition.Sequence <= 0)
                    throw new InvalidDataException("Transaction journal records require a nonzero owner and positive sequence.");
                if (transitions.TryGetValue(transition.Sequence, out var previous) && !previous.Equals(transition))
                    throw new InvalidDataException($"Conflicting transaction records for sequence {transition.Sequence}.");
                transitions[transition.Sequence] = transition;
                sequence = Math.Max(sequence, transition.Sequence);
            }
            else if (name.StartsWith(CommitMarkerPrefix, StringComparison.Ordinal))
            {
                if (!Guid.TryParseExact(name.Substring(CommitMarkerPrefix.Length), "N", out var owner) || owner == Guid.Empty)
                    throw new InvalidDataException("Malformed commit marker in journal.");
                if (rolledBack.Contains(owner))
                    throw new InvalidDataException($"Transaction {owner:N} has both commit and rollback decisions.");
                committed.Add(owner);
            }
            else if (name.StartsWith(RollbackMarkerPrefix, StringComparison.Ordinal))
            {
                if (!Guid.TryParseExact(name.Substring(RollbackMarkerPrefix.Length), "N", out var owner) || owner == Guid.Empty)
                    throw new InvalidDataException("Malformed rollback marker in journal.");
                if (committed.Contains(owner))
                    throw new InvalidDataException($"Transaction {owner:N} has both commit and rollback decisions.");
                rolledBack.Add(owner);
            }
            else if (name.StartsWith(AppliedMarkerPrefix, StringComparison.Ordinal))
            {
                if (!long.TryParse(name.Substring(AppliedMarkerPrefix.Length), NumberStyles.Integer, CultureInfo.InvariantCulture, out var seq) || seq <= 0)
                    throw new InvalidDataException("Malformed applied marker in journal.");
                applied.Add(seq);
                appliedSequence = Math.Max(appliedSequence, seq);
                // Retention may remove a payload while its checkpoint reserves the sequence.
                sequence = Math.Max(sequence, seq);
            }
        }
        return new RecoveryJournal(transitions.Values.OrderBy(t => t.Sequence).ToList(),
            committed, rolledBack, applied, sequence, appliedSequence);
    }
}
