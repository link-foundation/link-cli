using System.Globalization;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli;

public partial class VersionControlDecorator
{
    private const string MovementPrefix = "__vc:move:";
    internal sealed record MovementIntent(string FromBranch, long From, string ToBranch, long To,
        Transition<uint>[] Undo, Transition<uint>[] Redo);
    private sealed class MovementState(MovementIntent intent, uint link = 0, bool prepared = false)
    {
        internal readonly MovementIntent Intent = intent;
        internal uint Link = link;
        internal bool Prepared = prepared;
        internal bool RowsFlushed;
        internal bool Published;
    }
    private MovementState? _activeMovement;

    private static ulong MovementChecksum(string body)
    {
        var hash = 0xcbf29ce484222325UL;
        foreach (var value in body) hash = unchecked((hash ^ value) * 0x100000001b3UL);
        return hash;
    }

    internal static string EncodeMovement(MovementIntent intent)
    {
        static string Records(Transition<uint>[] records) => string.Join(',', records.Select(t => EncodeVersionName(t.Serialize())));
        var body = string.Join('|', "1", EncodeVersionName(intent.FromBranch),
            intent.From.ToString(CultureInfo.InvariantCulture), EncodeVersionName(intent.ToBranch),
            intent.To.ToString(CultureInfo.InvariantCulture), Records(intent.Undo), Records(intent.Redo));
        return MovementPrefix + body + "|" + MovementChecksum(body).ToString("x16", CultureInfo.InvariantCulture);
    }

    internal static bool TryDecodeMovement(string marker, out MovementIntent intent)
    {
        intent = null!;
        if (!marker.StartsWith(MovementPrefix, StringComparison.Ordinal)) return false;
        var text = marker[MovementPrefix.Length..];
        var separator = text.LastIndexOf('|');
        if (separator < 0) return false;
        var body = text[..separator];
        var hash = text[(separator + 1)..];
        if (hash.Length != 16 || !ulong.TryParse(hash, NumberStyles.AllowHexSpecifier,
            CultureInfo.InvariantCulture, out var expected) || MovementChecksum(body) != expected) return false;
        var fields = body.Split('|');
        if (fields.Length != 7 || fields[0] != "1"
            || !TryDecodeVersionName(fields[1], out var fromBranch) || !ValidMovementName(fromBranch)
            || !TryDecodeVersionName(fields[3], out var toBranch) || !ValidMovementName(toBranch)
            || !long.TryParse(fields[2], NumberStyles.None, CultureInfo.InvariantCulture, out var from)
            || !long.TryParse(fields[4], NumberStyles.None, CultureInfo.InvariantCulture, out var to)) return false;
        static bool Records(string text, out Transition<uint>[] records)
        {
            records = [];
            if (text.Length == 0) return true;
            var result = new List<Transition<uint>>();
            foreach (var encoded in text.Split(','))
            {
                if (!TryDecodeVersionName(encoded, out var payload) || !Transition<uint>.TryParse(payload, out var record)
                    || record.Serialize() != payload || record.TransactionId == Guid.Empty || record.Sequence <= 0) return false;
                var valid = record.Kind switch
                {
                    TransitionKind.Create => record.Before.Index == 0 && record.After.Index != 0,
                    TransitionKind.Update => record.Before.Index != 0 && record.Before.Index == record.After.Index,
                    TransitionKind.Delete => record.Before.Index != 0 && record.After.Index == 0,
                    _ => false,
                };
                if (!valid) return false;
                result.Add(record);
            }
            records = result.ToArray();
            return true;
        }
        if (!Records(fields[5], out var undo) || !Records(fields[6], out var redo)) return false;
        intent = new(fromBranch, from, toBranch, to, undo, redo);
        return EncodeMovement(intent) == marker;
    }

    private void StartMovementLocked(List<long> targetPath, string newBranch)
    {
        if (!ValidMovementName(_currentBranch) || !ValidMovementName(newBranch))
            throw new InvalidOperationException("Version movement requires nonblank branch names.");
        var to = targetPath.Count == 0 ? 0 : targetPath[^1];
        if (newBranch == _currentBranch && to == _currentApplied) return;
        var currentPath = BuildBranchSeqsLocked(_currentBranch).Where(s => s <= _currentApplied).ToList();
        var common = currentPath.Zip(targetPath).TakeWhile(pair => pair.First == pair.Second).Count();
        var available = _transactions.Log.ToDictionary(t => t.Sequence);
        Transition<uint> Required(long sequence) => available.TryGetValue(sequence, out var transition)
            ? transition : throw new InvalidOperationException($"Transition {sequence} is no longer retained; checkout requires complete changed history.");
        // Resolve all changed history before claiming ownership or changing rows.
        var undo = currentPath.Skip(common).Reverse().Select(Required).ToArray();
        var redo = targetPath.Skip(common).Select(Required).ToArray();
        _activeMovement = new(new(_currentBranch, _currentApplied, newBranch, to, undo, redo));
        FinishMovementLocked();
    }

    // Preserve the existing .NET branch API's complete UTF-16 string domain.
    private static bool ValidMovementName(string name) => !string.IsNullOrWhiteSpace(name);

    private void FinishMovementLocked()
    {
        if (_activeMovement is not { } state) return;
        if (!state.Prepared)
        {
            _transactions.BeginVersionReplay();
            var marker = EncodeMovement(state.Intent);
            if (state.Link == 0 || !_branchesStore.Exists(state.Link))
            {
                state.Link = _branchesStore.GetByName(marker);
                if (state.Link == 0) state.Link = _branchesStore.CreateAndUpdate(0u, 0u);
            }
            _branchesStore.SetName(state.Link, marker);
            _flushBranches();
            state.Prepared = true;
            Trace("Prepared durable version movement.");
        }
        if (!state.RowsFlushed)
        {
            foreach (var record in state.Intent.Undo) _transactions.ReplayVersionTransition(record, undo: true);
            foreach (var record in state.Intent.Redo) _transactions.ReplayVersionTransition(record, undo: false);
            _transactions.FlushVersionReplay();
            state.RowsFlushed = true;
        }
        if (!state.Published)
        {
            SetCurrentBranchLocked(state.Intent.ToBranch);
            SetAppliedLocked(state.Intent.To);
            _flushBranches();
            _currentBranch = state.Intent.ToBranch;
            _currentApplied = state.Intent.To;
            state.Published = true;
        }
        ClearPublicationIntentLocked(state.Link);
        _transactions.FinishVersionReplay();
        _activeMovement = null;
        Trace("Completed durable version movement.");
    }

    private void RecoverMovementsLocked(List<(uint Link, MovementIntent Intent)> pending)
    {
        if (pending.Count == 0) return;
        if (pending.Count != 1) throw new InvalidDataException("Multiple version movement intents.");
        var (link, intent) = pending[0];
        ValidateMovementAncestryLocked(intent.FromBranch);
        ValidateMovementAncestryLocked(intent.ToBranch);
        var fromPath = BuildBranchSeqsLocked(intent.FromBranch);
        var toPath = BuildBranchSeqsLocked(intent.ToBranch);
        if (!_branches.ContainsKey(intent.FromBranch) || !_branches.ContainsKey(intent.ToBranch)
            || (intent.From != 0 && !fromPath.Contains(intent.From)) || (intent.To != 0 && !toPath.Contains(intent.To))
            || (_currentBranch != intent.FromBranch && _currentBranch != intent.ToBranch)
            || (_currentApplied != intent.From && _currentApplied != intent.To)
            || (intent.FromBranch != intent.ToBranch && intent.From != intent.To
                && _currentBranch == intent.FromBranch && _currentApplied == intent.To))
            throw new InvalidDataException("Version movement intent contradicts selected history.");
        var from = fromPath.Where(s => s <= intent.From).ToArray();
        var to = toPath.Where(s => s <= intent.To).ToArray();
        var common = from.Zip(to).TakeWhile(pair => pair.First == pair.Second).Count();
        if (!intent.Undo.Select(t => t.Sequence).SequenceEqual(from.Skip(common).Reverse())
            || !intent.Redo.Select(t => t.Sequence).SequenceEqual(to.Skip(common)))
            throw new InvalidDataException("Version movement intent contradicts its transition paths.");
        var available = _transactions.Log.ToDictionary(t => t.Sequence);
        foreach (var record in intent.Undo.Concat(intent.Redo))
            if (!_transactions.IsTransactionCommitted(record.TransactionId) || _transactions.IsTransactionRolledBack(record.TransactionId)
                // The wire timestamp has millisecond precision; an in-memory
                // transition can retain finer DateTimeOffset ticks.
                || (available.TryGetValue(record.Sequence, out var stored) && stored.Serialize() != record.Serialize()))
                throw new InvalidDataException("Version movement intent contradicts its committed records.");
        _activeMovement = new(intent, link, prepared: true);
        _transactions.BeginVersionReplay();
        Trace("Recovering interrupted version movement.");
        FinishMovementLocked();
    }

    private void ValidateMovementAncestryLocked(string branch)
    {
        var visited = new HashSet<string>(StringComparer.Ordinal);
        string? current = branch;
        // Reject broken ancestry before resolving the movement paths.
        while (current is not null)
        {
            if (!visited.Add(current) || !_branches.TryGetValue(current, out var info)
                || info.ForkSeq < 0 || info.Head < info.ForkSeq)
                throw new InvalidDataException("Version movement has invalid branch ancestry.");
            current = info.Parent;
        }
    }
}
