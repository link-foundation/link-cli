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
        long AppliedSequence,
        List<TransactionUndoImage<TLinkAddress>> Images);

    // Build temporary indexes before touching live history, rows or markers.
    // Compare parsed values so idempotent retries keep legacy lexical forms.
    private RecoveryJournal ReadRecoveryJournal()
    {
        var transitions = new Dictionary<long, JournalRecord<TLinkAddress>>();
        var committed = new HashSet<Guid>();
        var rolledBack = new HashSet<Guid>();
        var applied = new HashSet<long>();
        var images = new List<TransactionUndoImage<TLinkAddress>>();
        Dictionary<Guid, TransactionUndoImage<TLinkAddress>>? imageOwners = null;
        long sequence = 0, appliedSequence = 0;
        var any = _logStore.Constants.Any;
        foreach (var raw in _logStore.All(new Link<TLinkAddress>(any, any, any)))
        {
            var name = _logStore.GetName(new Link<TLinkAddress>(raw).Index);
            if (string.IsNullOrEmpty(name)) continue;
            if (name.StartsWith(TransitionNamePrefix, StringComparison.Ordinal))
            {
                var payload = name.Substring(TransitionNamePrefix.Length);
                if (payload.StartsWith("v3|", StringComparison.Ordinal))
                {
                    var image = TransactionUndoImage<TLinkAddress>.Parse(payload);
                    imageOwners ??= new();
                    if (imageOwners.TryGetValue(image.Owner, out var previousImage))
                    {
                        if (previousImage.Serialize() != image.Serialize())
                            throw new InvalidDataException("Conflicting write-ahead undo images.");
                    }
                    else
                    {
                        images.Add(image);
                        imageOwners.Add(image.Owner, image);
                        sequence = Math.Max(sequence, image.Base);
                    }
                    continue;
                }
                if (!JournalRecord<TLinkAddress>.TryParse(payload, out var transition))
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
        var firstSequences = new Dictionary<Guid, long>();
        if (images.Count != 0)
            foreach (var record in transitions.Values)
                if (!firstSequences.TryGetValue(record.TransactionId, out var first) || record.Sequence < first)
                    firstSequences[record.TransactionId] = record.Sequence;
        var pendingBases = new HashSet<long>();
        foreach (var image in images)
        {
            if (firstSequences.TryGetValue(image.Owner, out var first) && first <= image.Base)
                throw new InvalidDataException("Undo image follows its owner's writes.");
            if (!committed.Contains(image.Owner) && !rolledBack.Contains(image.Owner) && !pendingBases.Add(image.Base))
                throw new InvalidDataException("Ambiguous pending undo images at the same base.");
        }
        return new RecoveryJournal(transitions.Values.OrderBy(t => t.Sequence).ToList(),
            committed, rolledBack, applied, sequence, appliedSequence, images);
    }
}
