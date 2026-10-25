using System.Globalization;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;
using Platform.Delegates;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli;

/// <summary>Metadata describing one branch in the version-control DAG.</summary>
public sealed record BranchInfo(string Name, string? Parent, long ForkSeq, long Head);

/// <summary>
/// Public surface of a links store wrapped with the version-control
/// decorator: same <see cref="INamedTypesLinks{TLinkAddress}"/> surface plus
/// branch/tag/checkout operations.
/// </summary>
public interface IVersionControlLinks : INamedTypesLinks<uint>
{
    string CurrentBranch { get; }
    long CurrentSequence { get; }
    ITransaction<uint> BeginTransaction();
    Task<ITransaction<uint>> BeginTransactionAsync(CancellationToken cancellationToken = default);
    IReadOnlyList<BranchInfo> ListBranches();
    IReadOnlyDictionary<string, long> ListTags();
    void Branch(string name, long? from = null);
    void SwitchBranch(string name);
    void Checkout(long sequence);
    void Tag(string name, long? sequence = null);
    bool TryGetTag(string name, out long sequence);
}

/// <summary>
/// Decorator that sits above <see cref="TransactionsDecorator"/> and
/// adds *time travel* (<see cref="Checkout"/>), *branching*
/// (<see cref="Branch"/>, <see cref="SwitchBranch"/>), and *tagging*
/// (<see cref="Tag"/>) over the transitions log. Optional — when not
/// instantiated the underlying transactions decorator behaves identically.
/// </summary>
public partial class VersionControlDecorator : LinksDecoratorBase<uint>, IVersionControlLinks, IDisposable
{
    /// <summary>Default name of the initial branch (analogous to git's <c>main</c>).</summary>
    public const string DefaultBranchName = "main";

    internal const string BranchPrefix = "__vc:branch:";
    internal const string TagPrefix = "__vc:tag:";
    internal const string CurrentPrefix = "__vc:current=";
    internal const string AppliedPrefix = "__vc:applied=";
    internal const string TransitionPrefix = "__vc:trans:";

    private readonly TransactionsDecorator _transactions;
    private readonly INamedTypesLinks<uint> _branchesStore;
    private readonly object _lock = new();
    private readonly Dictionary<string, BranchInfo> _branches = new(StringComparer.Ordinal);
    private readonly Dictionary<string, long> _tags = new(StringComparer.Ordinal);
    private readonly Dictionary<long, string> _transitionBranches = new();
    private readonly Dictionary<string, uint> _branchLinks = new(StringComparer.Ordinal);
    private readonly Dictionary<string, uint> _tagLinks = new(StringComparer.Ordinal);
    private uint _currentBranchLink;
    private uint _appliedLink;
    private string _currentBranch = DefaultBranchName;
    private long _currentApplied;
    private VersionControlTransaction? _activeTransaction;
    private readonly bool _trace;
    private readonly Action _flushBranches;

    /// <summary>Both the transaction decision and version sidecar have explicit stable-storage flushes.</summary>
    public bool SupportsDurablePublication { get; }

    /// <summary>
    /// Completes a committed version publication or rolls back an open transaction.
    /// The wrapped transactions decorator and branches store are owned by
    /// the caller and are deliberately left untouched.
    /// </summary>
    public void Dispose()
    {
        Dispose(true);
        GC.SuppressFinalize(this);
    }

    /// <summary>
    /// Finishes the active version transaction. Derived decorators that
    /// own extra resources override this and call
    /// <c>base.Dispose(disposing)</c>.
    /// </summary>
    protected virtual void Dispose(bool disposing)
    {
        if (!disposing) return;
        VersionControlTransaction? active;
        lock (_lock)
        {
            FinishMovementLocked();
            active = _activeTransaction;
        }
        active?.Dispose();
    }

    public VersionControlDecorator(
      TransactionsDecorator transactions,
      INamedTypesLinks<uint> branchesStore,
      bool trace = false)
      : this(transactions, branchesStore, null, trace)
    {
    }

    /// <summary>Uses an explicit stable-storage flush for caller-owned metadata.</summary>
    public VersionControlDecorator(
      TransactionsDecorator transactions,
      INamedTypesLinks<uint> branchesStore,
      Action? flushBranches,
      bool trace = false)
      : base(transactions)
    {
        _transactions = transactions ?? throw new ArgumentNullException(nameof(transactions));
        _branchesStore = branchesStore ?? throw new ArgumentNullException(nameof(branchesStore));
        _trace = trace;
        var durable = branchesStore as IDurableLinks;
        SupportsDurablePublication = transactions.SupportsDurableCommit
            && (flushBranches is not null || durable?.SupportsDurableFlush == true);
        _flushBranches = flushBranches ?? (durable?.SupportsDurableFlush == true ? durable.Flush : static () => { });
        try
        {
            _transactions.ResumeVersionReplayRecovery();
            Recover();
            EnsureDefaultBranch();
        }
        catch
        {
            if (_activeMovement is not null)
                _transactions.RetainVersionReplayRecovery(() => { lock (_lock) FinishMovementLocked(); });
            throw;
        }
    }

    public virtual string CurrentBranch { get { lock (_lock) return _currentBranch; } }
    public virtual long CurrentSequence { get { lock (_lock) return _currentApplied; } }

    public virtual IReadOnlyList<BranchInfo> ListBranches()
    {
        lock (_lock) return _branches.Values.OrderBy(b => b.Name, StringComparer.Ordinal).ToArray();
    }

    public virtual IReadOnlyDictionary<string, long> ListTags()
    {
        lock (_lock) return new Dictionary<string, long>(_tags, StringComparer.Ordinal);
    }

    public virtual bool TryGetTag(string name, out long sequence)
    {
        lock (_lock) return _tags.TryGetValue(name, out sequence);
    }

    public virtual ITransaction<uint> BeginTransaction()
    {
        lock (_lock)
        {
            if (_activeTransaction is not null || _activeMovement is not null)
            {
                throw new InvalidOperationException("Nested version-control transactions are not supported.");
            }

            var beforeSequence = _transactions.LastLoggedSequence;
            var branchName = _currentBranch;
            var inner = _transactions.BeginTransaction();
            _activeTransaction = new VersionControlTransaction(this, inner, branchName, beforeSequence);
            return _activeTransaction;
        }
    }

    public virtual Task<ITransaction<uint>> BeginTransactionAsync(CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(BeginTransaction());
    }

    // -- Write overrides (attribute new transitions to the current branch) --

    public override uint Create(IList<uint>? substitution, WriteHandler<uint>? handler)
    {
        return RunVcWrite(() => _transactions.Create(substitution, handler));
    }

    public override uint Update(IList<uint>? restriction, IList<uint>? substitution, WriteHandler<uint>? handler)
    {
        return RunVcWrite(() => _transactions.Update(restriction, substitution, handler));
    }

    public override uint Delete(IList<uint>? restriction, WriteHandler<uint>? handler)
    {
        return RunVcWrite(() => _transactions.Delete(restriction, handler));
    }

    private uint RunVcWrite(Func<uint> innerWrite)
    {
        lock (_lock)
        {
            EnsureWriteAllowedLocked();
            // An automatic write needs the same publication owner as an
            // explicit transaction, including when its commit throws.
            var automatic = _activeTransaction is null ? BeginTransaction() : null;
            uint result;
            try
            {
                result = innerWrite();
            }
            catch
            {
                // Errors before commit still undo an automatic write. Failed
                // undo retains this owner for a subsequent Dispose retry.
                automatic?.Rollback();
                throw;
            }
            // Keep this outside the undo catch: an attempted commit can be
            // durable even when it reports an error, and must be completed.
            automatic?.Commit();
            return result;
        }
    }

    private void EnsureWriteAllowedLocked()
    {
        if (_activeMovement is not null || _activeTransaction?.AfterSequence is not null || _activeTransaction?.Inner.IsCommitted == true)
            throw new InvalidOperationException("Retry committed version publication before writing.");
    }

    private void AttributeNewTransitionsLocked(long beforeSeq, long afterSeq, string branchName)
    {
        if (afterSeq <= beforeSeq) return;

        for (var s = beforeSeq; s < afterSeq;)
        {
            ++s;
            _transitionBranches[s] = branchName;
            WriteImmutableMarker($"{TransitionPrefix}{s.ToString(CultureInfo.InvariantCulture)}:branch={branchName}");
        }
        if (_branches.TryGetValue(branchName, out var info))
        {
            var updated = info with { Head = afterSeq };
            _branches[branchName] = updated;
            UpdateBranchLinkLocked(updated);
        }
        if (string.Equals(_currentBranch, branchName, StringComparison.Ordinal))
        {
            _currentApplied = afterSeq;
            SetAppliedLocked(afterSeq);
        }
    }

    // -- Branching ---------------------------------------------------------

    public virtual void Branch(string name, long? from = null)
    {
        if (string.IsNullOrWhiteSpace(name))
        {
            throw new ArgumentException("Branch name must not be empty.", nameof(name));
        }
        lock (_lock)
        {
            EnsureNoOpenTransactionLocked(nameof(Branch));
            if (_branches.ContainsKey(name))
            {
                throw new InvalidOperationException($"Branch '{name}' already exists.");
            }
            var parent = _currentBranch;
            var forkSeq = from ?? _currentApplied;
            if (forkSeq < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(from), forkSeq, "Fork point cannot be negative.");
            }
            if (forkSeq > 0)
            {
                var path = BuildBranchSeqsLocked(parent);
                if (!path.Contains(forkSeq))
                {
                    throw new InvalidOperationException($"Fork point {forkSeq} is not reachable on branch '{parent}'.");
                }
            }
            CreateBranchLocked(name, parent, forkSeq, head: forkSeq);
            Trace($"Created branch '{name}' from '{parent}' at seq {forkSeq}.");
        }
    }

    public virtual void SwitchBranch(string name)
    {
        lock (_lock)
        {
            EnsureNoOpenTransactionLocked(nameof(SwitchBranch));
            if (!_branches.TryGetValue(name, out var target))
            {
                throw new InvalidOperationException($"Unknown branch '{name}'.");
            }
            var targetPath = BuildBranchSeqsLocked(name);
            ApplyDiffToLocked(targetPath, newBranch: name);
            Trace($"Switched to branch '{name}' at seq {_currentApplied}.");
        }
    }

    public virtual void Checkout(long sequence)
    {
        lock (_lock)
        {
            EnsureNoOpenTransactionLocked(nameof(Checkout));
            if (sequence < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(sequence), sequence, "Sequence must be non-negative.");
            }
            var path = BuildBranchSeqsLocked(_currentBranch);
            if (sequence > 0 && !path.Contains(sequence))
            {
                throw new InvalidOperationException($"Sequence {sequence} is not reachable on branch '{_currentBranch}'.");
            }
            ApplyDiffToLocked(path.Where(s => s <= sequence).ToList(), newBranch: _currentBranch);
            Trace($"Checked out seq {sequence} on branch '{_currentBranch}'.");
        }
    }

    public virtual void Tag(string name, long? sequence = null)
    {
        if (string.IsNullOrWhiteSpace(name))
        {
            throw new ArgumentException("Tag name must not be empty.", nameof(name));
        }
        lock (_lock)
        {
            EnsureNoOpenTransactionLocked(nameof(Tag));
            var seq = sequence ?? _currentApplied;
            if (seq < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(sequence), seq, "Tag sequence must be non-negative.");
            }
            _tags[name] = seq;
            UpdateTagLinkLocked(name, seq);
            Trace($"Created tag '{name}' at seq {seq}.");
        }
    }

    // -- Path / diff helpers ----------------------------------------------

    private void ApplyDiffToLocked(List<long> targetPath, string newBranch)
    {
        StartMovementLocked(targetPath, newBranch);
    }

    private void EnsureNoOpenTransactionLocked(string operation)
    {
        if (_activeTransaction is not null || _activeMovement is not null || _transactions.HasOpenTransaction)
        {
            throw new InvalidOperationException($"{operation} is not allowed while a version-control transaction is open.");
        }
    }

    private void CommitVersionTransaction(VersionControlTransaction transaction)
    {
        lock (_lock)
        {
            if (!ReferenceEquals(_activeTransaction, transaction))
            {
                transaction.Inner.Commit();
                return;
            }
            if (transaction.Inner.IsRolledBack)
            {
                FinishAbortedPublicationLocked(transaction);
                return;
            }
            PreparePublicationLocked(transaction);
            // IsCommitted reports the durable decision; a failed data checkpoint
            // can still need retry before publishing the branch metadata.
            transaction.Inner.Commit();
            if (ReferenceEquals(_activeTransaction, transaction))
            {
                AttributeNewTransitionsLocked(transaction.BeforeSequence, transaction.AfterSequence!.Value, transaction.BranchName);
                _flushBranches();
                ClearPublicationIntentLocked(transaction.IntentLink);
                _activeTransaction = null;
                if (_trace) Trace($"Published committed version tx {transaction.Id:N} on '{transaction.BranchName}'.");
            }
        }
    }

    private void RollbackVersionTransaction(VersionControlTransaction transaction)
    {
        lock (_lock)
        {
            if (transaction.Inner.IsCommitted)
            {
                CommitVersionTransaction(transaction);
                return;
            }
            try
            {
                transaction.Inner.Rollback();
                // An uncertain inner decision can become known while cleanup
                // retries it. Publish that commit before releasing admission.
                if (transaction.Inner.IsCommitted) CommitVersionTransaction(transaction);
            }
            finally
            {
                // Failed undo/checkpoint work still owns admission and must be
                // retryable through this facade. Retention can fail after the
                // inner rollback completes, so inspect its terminal state.
                if (transaction.Inner.IsRolledBack
                    && ReferenceEquals(_activeTransaction, transaction))
                {
                    FinishAbortedPublicationLocked(transaction);
                }
            }
        }
    }

    private List<long> BuildBranchSeqsLocked(string branchName)
    {
        // Only descendant fork points constrain inherited records. Each own
        // head applies separately, preserving root-to-leaf history grouping.
        var positions = new Dictionary<string, int>(StringComparer.Ordinal);
        var chain = new List<(long Limit, List<long> Sequences)>();
        string? current = branchName;
        var cutoff = long.MaxValue;
        while (current is not null && _branches.TryGetValue(current, out var info)
            && positions.TryAdd(current, chain.Count))
        {
            chain.Add((Math.Min(cutoff, info.Head), new List<long>()));
            cutoff = Math.Min(cutoff, info.ForkSeq);
            current = info.Parent;
        }
        // Partition in one attribution scan; only selected buckets need sorting.
        foreach (var (sequence, owner) in _transitionBranches)
            if (positions.TryGetValue(owner, out var position))
            {
                var entry = chain[position];
                if (sequence <= entry.Limit) entry.Sequences.Add(sequence);
            }
        var result = new List<long>();
        for (var position = chain.Count - 1; position >= 0; --position)
        {
            var sequences = chain[position].Sequences;
            sequences.Sort();
            result.AddRange(sequences);
        }
        return result;
    }

    // -- Persistence helpers ----------------------------------------------

    private void EnsureDefaultBranch()
    {
        lock (_lock)
        {
            if (!_branches.ContainsKey(DefaultBranchName))
            {
                var existing = 0L;
                // Only committed records are versions. Aborts and recovered
                // pending work can leave real sequence numbers in the journal.
                foreach (var transition in _transactions.Log)
                {
                    if (!_transactions.IsTransactionCommitted(transition.TransactionId)) continue;
                    var s = transition.Sequence;
                    existing = Math.Max(existing, s);
                    if (!_transitionBranches.ContainsKey(s))
                    {
                        _transitionBranches[s] = DefaultBranchName;
                        WriteImmutableMarker($"{TransitionPrefix}{s.ToString(CultureInfo.InvariantCulture)}:branch={DefaultBranchName}");
                    }
                }
                CreateBranchLocked(DefaultBranchName, parent: null, forkSeq: 0, head: existing);
                _currentBranch = DefaultBranchName;
                _currentApplied = existing;
                SetCurrentBranchLocked(DefaultBranchName);
                SetAppliedLocked(existing);
            }
            else if (_currentBranchLink == 0)
            {
                SetCurrentBranchLocked(_currentBranch);
            }
        }
    }

    private void CreateBranchLocked(string name, string? parent, long forkSeq, long head)
    {
        var info = new BranchInfo(name, parent, forkSeq, head);
        _branches[name] = info;
        UpdateBranchLinkLocked(info);
    }

    private void UpdateBranchLinkLocked(BranchInfo info)
    {
        var nameMarker = EncodeBranchMarker(info);
        if (!_branchLinks.TryGetValue(info.Name, out var link))
        {
            link = _branchesStore.CreateAndUpdate(_branchesStore.Constants.Null, _branchesStore.Constants.Null);
            _branchLinks[info.Name] = link;
        }
        _branchesStore.SetName(link, nameMarker);
    }

    private void UpdateTagLinkLocked(string name, long seq)
    {
        var nameMarker = $"{TagPrefix}{name}={seq.ToString(CultureInfo.InvariantCulture)}";
        if (!_tagLinks.TryGetValue(name, out var link))
        {
            link = _branchesStore.CreateAndUpdate(_branchesStore.Constants.Null, _branchesStore.Constants.Null);
            _tagLinks[name] = link;
        }
        _branchesStore.SetName(link, nameMarker);
    }

    private void SetCurrentBranchLocked(string name)
    {
        _currentBranch = name;
        if (_currentBranchLink == 0)
        {
            _currentBranchLink = _branchesStore.CreateAndUpdate(_branchesStore.Constants.Null, _branchesStore.Constants.Null);
        }
        _branchesStore.SetName(_currentBranchLink, $"{CurrentPrefix}{name}");
    }

    private void SetAppliedLocked(long seq)
    {
        if (_appliedLink == 0)
        {
            _appliedLink = _branchesStore.CreateAndUpdate(_branchesStore.Constants.Null, _branchesStore.Constants.Null);
        }
        _branchesStore.SetName(_appliedLink, $"{AppliedPrefix}{seq.ToString(CultureInfo.InvariantCulture)}");
    }

    private void WriteImmutableMarker(string name)
    {
        // Publication may be retried after SetName changed storage and threw.
        if (_branchesStore.GetByName(name) != 0) return;
        var link = _branchesStore.CreateAndUpdate(_branchesStore.Constants.Null, _branchesStore.Constants.Null);
        _branchesStore.SetName(link, name);
    }

    internal static string EncodeBranchMarker(BranchInfo info)
    {
        return string.Concat(BranchPrefix, "2|", EncodeVersionName(info.Name), "|",
          info.Parent is null ? "-" : EncodeVersionName(info.Parent), "|",
          info.ForkSeq.ToString(CultureInfo.InvariantCulture), "|",
          info.Head.ToString(CultureInfo.InvariantCulture));
    }

    // Four hex digits per UTF-16 code unit avoid delimiter ambiguity and retain
    // the exact .NET string, including supplementary characters and NULs.
    private static string EncodeVersionName(string value)
    {
        const string digits = "0123456789ABCDEF";
        return string.Create(checked(value.Length * 4), value, (output, name) =>
        {
            for (var i = 0; i < name.Length; ++i)
                for (var j = 0; j < 4; ++j)
                    output[i * 4 + j] = digits[(name[i] >> ((3 - j) * 4)) & 15];
        });
    }

    private static bool TryDecodeVersionName(string text, out string name)
    {
        name = string.Empty;
        if (text.Length % 4 != 0) return false;
        var units = new char[text.Length / 4];
        for (var i = 0; i < units.Length; ++i)
        {
            if (!ushort.TryParse(text.AsSpan(i * 4, 4), NumberStyles.AllowHexSpecifier,
                CultureInfo.InvariantCulture, out var unit)) return false;
            units[i] = (char)unit;
        }
        name = new string(units);
        return true;
    }

    internal static bool TryDecodeBranchMarker(string text, out BranchInfo info)
    {
        info = default!;
        if (!text.StartsWith(BranchPrefix, StringComparison.Ordinal)) return false;
        var rest = text.Substring(BranchPrefix.Length);
        if (rest.StartsWith("2|", StringComparison.Ordinal))
        {
            var fields = rest.Split('|');
            if (fields.Length == 5 && TryDecodeVersionName(fields[1], out var decodedName)
                && long.TryParse(fields[3], NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var decodedFork)
                && long.TryParse(fields[4], NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var decodedHead))
            {
                string? parent = null;
                if (fields[2] != "-")
                {
                    if (!TryDecodeVersionName(fields[2], out var decodedParent)) return false;
                    parent = decodedParent;
                }
                info = new BranchInfo(decodedName, parent, decodedFork, decodedHead);
                return true;
            }
            // A legacy branch name may itself start with "2|". Its delimiter
            // suffix distinguishes it from a valid version-two marker.
        }
        var parentIdx = rest.IndexOf(":parent=", StringComparison.Ordinal);
        if (parentIdx < 0) return false;
        var name = rest.Substring(0, parentIdx);
        rest = rest.Substring(parentIdx + ":parent=".Length);
        var forkIdx = rest.IndexOf(":fork=", StringComparison.Ordinal);
        if (forkIdx < 0) return false;
        var parentText = rest.Substring(0, forkIdx);
        rest = rest.Substring(forkIdx + ":fork=".Length);
        var headIdx = rest.IndexOf(":head=", StringComparison.Ordinal);
        if (headIdx < 0) return false;
        var forkText = rest.Substring(0, headIdx);
        var headText = rest.Substring(headIdx + ":head=".Length);
        if (!long.TryParse(forkText, NumberStyles.Integer, CultureInfo.InvariantCulture, out var fork)) return false;
        if (!long.TryParse(headText, NumberStyles.Integer, CultureInfo.InvariantCulture, out var head)) return false;
        info = new BranchInfo(name, parentText.Length == 0 ? null : parentText, fork, head);
        return true;
    }

    public virtual void Recover()
    {
        lock (_lock)
        {
            EnsureNoOpenTransactionLocked("Recover");
            var uncommitted = _transactions.Log
                .Where(t => !_transactions.IsTransactionCommitted(t.TransactionId))
                .Select(t => t.Sequence).ToHashSet();
            _branches.Clear();
            _tags.Clear();
            _transitionBranches.Clear();
            _branchLinks.Clear();
            _tagLinks.Clear();
            _currentBranch = DefaultBranchName;
            _currentBranchLink = 0;
            _appliedLink = 0;
            _currentApplied = 0;
            var pending = new List<(uint Link, PublicationIntent Intent)>();
            var movements = new List<(uint Link, MovementIntent Intent)>();
            var ambiguousMovementMetadata = false;

            var any = _branchesStore.Constants.Any;
            var anyLink = new DoubletLink(any, any, any);
            foreach (var raw in _branchesStore.All(anyLink))
            {
                var link = new DoubletLink(raw);
                var name = _branchesStore.GetName(link.Index);
                if (string.IsNullOrEmpty(name)) continue;

                if (name.StartsWith(MovementPrefix, StringComparison.Ordinal))
                {
                    if (!TryDecodeMovement(name, out var movement))
                        throw new InvalidDataException("Invalid version movement intent.");
                    movements.Add((link.Index, movement));
                }
                else if (name.StartsWith(PublicationPrefix, StringComparison.Ordinal))
                {
                    if (!TryDecodePublicationMarker(name, out var intent))
                        throw new InvalidDataException("Invalid version publication intent.");
                    pending.Add((link.Index, intent));
                }
                else if (name.StartsWith(BranchPrefix, StringComparison.Ordinal))
                {
                    if (TryDecodeBranchMarker(name, out var info))
                    {
                        ambiguousMovementMetadata |= _branches.ContainsKey(info.Name);
                        _branches[info.Name] = info;
                        _branchLinks[info.Name] = link.Index;
                    }
                    else ambiguousMovementMetadata = true;
                }
                else if (name.StartsWith(CurrentPrefix, StringComparison.Ordinal))
                {
                    ambiguousMovementMetadata |= _currentBranchLink != 0;
                    _currentBranch = name.Substring(CurrentPrefix.Length);
                    _currentBranchLink = link.Index;
                }
                else if (name.StartsWith(AppliedPrefix, StringComparison.Ordinal))
                {
                    var rest = name.Substring(AppliedPrefix.Length);
                    if (long.TryParse(rest, NumberStyles.Integer, CultureInfo.InvariantCulture, out var seq))
                    {
                        ambiguousMovementMetadata |= _appliedLink != 0 || seq < 0;
                        _currentApplied = seq;
                        _appliedLink = link.Index;
                    }
                    else ambiguousMovementMetadata = true;
                }
                else if (name.StartsWith(TagPrefix, StringComparison.Ordinal))
                {
                    var rest = name.Substring(TagPrefix.Length);
                    var eq = rest.LastIndexOf('=');
                    if (eq > 0)
                    {
                        var tagName = rest.Substring(0, eq);
                        if (long.TryParse(rest.Substring(eq + 1), NumberStyles.Integer, CultureInfo.InvariantCulture, out var tagSeq))
                        {
                            _tags[tagName] = tagSeq;
                            _tagLinks[tagName] = link.Index;
                        }
                    }
                }
                else if (name.StartsWith(TransitionPrefix, StringComparison.Ordinal))
                {
                    var rest = name.Substring(TransitionPrefix.Length);
                    var colon = rest.IndexOf(":branch=", StringComparison.Ordinal);
                    if (colon > 0 &&
                        long.TryParse(rest.Substring(0, colon), NumberStyles.Integer, CultureInfo.InvariantCulture, out var seq))
                    {
                        if (uncommitted.Contains(seq))
                            throw new InvalidDataException($"Version metadata references uncommitted transition {seq}.");
                        var branchName = rest.Substring(colon + ":branch=".Length);
                        ambiguousMovementMetadata |= seq <= 0 || _transitionBranches.ContainsKey(seq);
                        _transitionBranches[seq] = branchName;
                    }
                    else ambiguousMovementMetadata = true;
                }
            }
            if (pending.Count != 0 && movements.Count != 0)
                throw new InvalidDataException("Version publication and movement intents overlap.");
            if (movements.Count != 0 && (ambiguousMovementMetadata || _currentBranchLink == 0 || _appliedLink == 0))
                throw new InvalidDataException("Ambiguous version movement metadata.");
            RecoverPublicationsLocked(pending);
            RecoverMovementsLocked(movements);
        }
    }

    // -- INamedTypes forwarding -------------------------------------------

    public virtual bool ReassignsNames => _transactions.ReassignsNames;
    public virtual IReadOnlyDictionary<uint, string> CaptureNames() => _transactions.CaptureNames();
    public virtual string? GetName(uint link) => _transactions.GetName(link);
    public virtual uint SetName(uint link, string name)
    {
        return RunVcWrite(() => _transactions.SetName(link, name));
    }
    public virtual uint GetByName(string name) => _transactions.GetByName(name);
    public virtual void RemoveName(uint link)
    {
        RunVcWrite(() => { _transactions.RemoveName(link); return 0u; });
    }

    // -- Convenience ------------------------------------------------------

    /// <summary>Conventional sidecar filename for the version-control store.</summary>
    public static string MakeVersionControlDatabaseFilename(string databaseFilename)
    {
        ArgumentNullException.ThrowIfNull(databaseFilename);
        var filenameWithoutExtension = Path.GetFileNameWithoutExtension(databaseFilename);
        var directory = Path.GetDirectoryName(databaseFilename);
        return Path.Combine(directory ?? string.Empty, $"{filenameWithoutExtension}.versioncontrol.links");
    }

    private void Trace(string message)
    {
        if (_trace) DiagnosticOutput.WriteLine($"[VersionControl] {message}");
    }

    private sealed class VersionControlTransaction : ITransaction<uint>
    {
        private readonly VersionControlDecorator _owner;

        internal VersionControlTransaction(
          VersionControlDecorator owner,
          ITransaction<uint> inner,
          string branchName,
          long beforeSequence)
        {
            _owner = owner;
            Inner = inner;
            BranchName = branchName;
            BeforeSequence = beforeSequence;
        }

        internal ITransaction<uint> Inner { get; }
        internal string BranchName { get; }
        internal long BeforeSequence { get; }
        internal long? AfterSequence { get; set; }
        internal uint IntentLink { get; set; }

        public Guid Id => Inner.Id;
        public DateTimeOffset StartedAt => Inner.StartedAt;
        public bool IsCommitted => Inner.IsCommitted;
        public bool IsRolledBack => Inner.IsRolledBack;
        public IReadOnlyList<Transition<uint>> Transitions => Inner.Transitions;

        public void Commit() => _owner.CommitVersionTransaction(this);

        public Task CommitAsync(CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            _owner.CommitVersionTransaction(this);
            return Task.CompletedTask;
        }

        public void Rollback() => _owner.RollbackVersionTransaction(this);

        public void Dispose()
        {
            if (Inner.IsCommitted)
            {
                // The journal decision can precede a failed data checkpoint.
                // Finish version publication without undoing that commit.
                _owner.CommitVersionTransaction(this);
            }
            else
            {
                _owner.RollbackVersionTransaction(this);
            }
        }
    }
}
