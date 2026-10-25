using System.Globalization;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli;

public partial class VersionControlDecorator
{
    internal const string PublicationPrefix = "__vc:pending:";
    internal sealed record PublicationIntent(Guid Id, long Before, long After, string Branch);

    internal static string EncodePublicationMarker(PublicationIntent intent) => string.Concat(
        PublicationPrefix, "2|", intent.Id.ToString("N"), "|",
        intent.Before.ToString(CultureInfo.InvariantCulture), "|",
        intent.After.ToString(CultureInfo.InvariantCulture), "|", EncodeVersionName(intent.Branch));

    internal static bool TryDecodePublicationMarker(string marker, out PublicationIntent intent)
    {
        intent = default!;
        if (!marker.StartsWith(PublicationPrefix, StringComparison.Ordinal)) return false;
        var fields = marker.Substring(PublicationPrefix.Length).Split('|');
        if (fields.Length != 5 || fields[0] != "2" || fields[1].Length != 32
            || !Guid.TryParseExact(fields[1], "N", out var id) || id == Guid.Empty
            || fields[2].Any(c => c is < '0' or > '9') || fields[3].Any(c => c is < '0' or > '9')
            || !long.TryParse(fields[2], NumberStyles.None, CultureInfo.InvariantCulture, out var before)
            || !long.TryParse(fields[3], NumberStyles.None, CultureInfo.InvariantCulture, out var after)
            || after < before || !TryDecodeVersionName(fields[4], out var branch)
            || string.IsNullOrWhiteSpace(branch)) return false;
        intent = new PublicationIntent(id, before, after, branch);
        return true;
    }

    private void PreparePublicationLocked(VersionControlTransaction transaction)
    {
        // Fence the exact range before the first fallible metadata operation.
        transaction.AfterSequence ??= _transactions.LastLoggedSequence;
        var intent = new PublicationIntent(transaction.Id, transaction.BeforeSequence,
            transaction.AfterSequence.Value, transaction.BranchName);
        var marker = EncodePublicationMarker(intent);
        if (transaction.IntentLink == 0 || !_branchesStore.Exists(transaction.IntentLink))
        {
            transaction.IntentLink = _branchesStore.GetByName(marker);
            if (transaction.IntentLink == 0)
                transaction.IntentLink = _branchesStore.CreateAndUpdate(0u, 0u);
        }
        if (!string.Equals(_branchesStore.GetName(transaction.IntentLink), marker, StringComparison.Ordinal))
            _branchesStore.SetName(transaction.IntentLink, marker);
        // This also persists the selected branch and its ancestry before the
        // journal decision can require publication after an abrupt exit.
        _flushBranches();
        if (_trace) Trace($"Prepared version publication {intent.Id:N}: {intent.Before}..{intent.After} on '{intent.Branch}'.");
    }

    private void ClearPublicationIntentLocked(uint link)
    {
        if (link != 0)
        {
            _branchesStore.RemoveName(link);
            if (_branchesStore.Exists(link)) _branchesStore.Delete(new[] { link }, null);
        }
        // A failed final flush still owns admission, even if the name/deletion
        // already took effect. Repeating cleanup must flush again.
        _flushBranches();
    }

    private void FinishAbortedPublicationLocked(VersionControlTransaction transaction)
    {
        if (transaction.AfterSequence is not null) ClearPublicationIntentLocked(transaction.IntentLink);
        if (ReferenceEquals(_activeTransaction, transaction)) _activeTransaction = null;
    }

    private void RecoverPublicationsLocked(List<(uint Link, PublicationIntent Intent)> pending)
    {
        if (pending.Count == 0) return;
        if (pending.Count != 1) throw new InvalidDataException("Multiple version publication intents.");
        var (link, intent) = pending[0];
        if (!_branches.TryGetValue(intent.Branch, out var branch)
            || !string.Equals(_currentBranch, intent.Branch, StringComparison.Ordinal)
            || branch.Head > intent.After || _currentApplied > intent.After)
            throw new InvalidDataException("Version publication intent contradicts branch metadata.");

        var committed = _transactions.IsTransactionCommitted(intent.Id);
        if (committed && _transactions.IsTransactionRolledBack(intent.Id))
            throw new InvalidDataException("Version publication has conflicting transaction decisions.");
        var records = _transactions.Log.Where(t => t.Sequence > intent.Before && t.Sequence <= intent.After).ToArray();
        if (records.Any(t => t.TransactionId != intent.Id)
            || _transitionBranches.Any(p => p.Key > intent.Before && p.Key <= intent.After
                && !string.Equals(p.Value, intent.Branch, StringComparison.Ordinal))
            || (committed && (long)records.Length != intent.After - intent.Before))
            throw new InvalidDataException("Version publication intent contradicts its transition range.");
        if (committed)
        {
            AttributeNewTransitionsLocked(intent.Before, intent.After, intent.Branch);
            _flushBranches();
        }
        // The transaction constructor already durably redoes committed records
        // and undoes pending records before this sidecar recovery is entered.
        ClearPublicationIntentLocked(link);
        if (_trace) Trace($"Recovered {(committed ? "committed" : "aborted")} version publication {intent.Id:N}.");
    }
}
