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
public class VersionControlDecorator : LinksDecoratorBase<uint>, IVersionControlLinks, IDisposable
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

    /// <summary>
    /// Rolls back and releases the transaction that is still open, if any.
    /// The wrapped transactions decorator and branches store are owned by
    /// the caller and are deliberately left untouched.
    /// </summary>
    public void Dispose()
    {
        Dispose(true);
        GC.SuppressFinalize(this);
    }

    /// <summary>
    /// Rolls back and releases the open transaction. Derived decorators that
    /// own extra resources override this and call
    /// <c>base.Dispose(disposing)</c>.
    /// </summary>
    protected virtual void Dispose(bool disposing)
    {
        if (!disposing) return;
        VersionControlTransaction? active;
        lock (_lock)
        {
            active = _activeTransaction;
            _activeTransaction = null;
        }
        active?.Dispose();
    }

    public VersionControlDecorator(
      TransactionsDecorator transactions,
      INamedTypesLinks<uint> branchesStore,
      bool trace = false)
      : base(transactions)
    {
        _transactions = transactions ?? throw new ArgumentNullException(nameof(transactions));
        _branchesStore = branchesStore ?? throw new ArgumentNullException(nameof(branchesStore));
        _trace = trace;
        Recover();
        EnsureDefaultBranch();
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
            if (_activeTransaction is not null)
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
            var beforeSeq = _transactions.LastLoggedSequence;
            var result = innerWrite();
            if (_activeTransaction is null)
            {
                AttributeNewTransitionsLocked(beforeSeq, _currentBranch);
            }
            return result;
        }
    }

    private void AttributeNewTransitionsLocked(long beforeSeq, string branchName)
    {
        var afterSeq = _transactions.LastLoggedSequence;
        if (afterSeq <= beforeSeq) return;

        for (var s = beforeSeq + 1; s <= afterSeq; s++)
        {
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
        var currentPath = BuildBranchSeqsLocked(_currentBranch)
          .Where(s => s <= _currentApplied)
          .ToList();

        var common = 0;
        var max = Math.Min(currentPath.Count, targetPath.Count);
        while (common < max && currentPath[common] == targetPath[common]) common++;

        for (var i = currentPath.Count - 1; i >= common; i--)
        {
            var transition = FindTransition(currentPath[i]);
            if (transition is not null)
            {
                _transactions.RevertTransition(transition.Value);
            }
        }
        for (var i = common; i < targetPath.Count; i++)
        {
            var transition = FindTransition(targetPath[i]);
            if (transition is not null)
            {
                _transactions.ApplyTransition(transition.Value);
            }
        }

        if (!ReferenceEquals(newBranch, _currentBranch))
        {
            _currentBranch = newBranch;
            SetCurrentBranchLocked(newBranch);
        }
        _currentApplied = targetPath.Count == 0 ? 0 : targetPath[^1];
        SetAppliedLocked(_currentApplied);
    }

    private void EnsureNoOpenTransactionLocked(string operation)
    {
        if (_activeTransaction is not null)
        {
            throw new InvalidOperationException($"{operation} is not allowed while a version-control transaction is open.");
        }
    }

    private void CommitVersionTransaction(VersionControlTransaction transaction)
    {
        lock (_lock)
        {
            transaction.Inner.Commit();
            if (ReferenceEquals(_activeTransaction, transaction))
            {
                _activeTransaction = null;
                AttributeNewTransitionsLocked(transaction.BeforeSequence, transaction.BranchName);
            }
        }
    }

    private void RollbackVersionTransaction(VersionControlTransaction transaction)
    {
        lock (_lock)
        {
            try
            {
                transaction.Inner.Rollback();
            }
            finally
            {
                if (ReferenceEquals(_activeTransaction, transaction))
                {
                    _activeTransaction = null;
                }
            }
        }
    }

    private List<long> BuildBranchSeqsLocked(string branchName)
    {
        return BuildBranchSeqsLocked(branchName, new HashSet<string>(StringComparer.Ordinal));
    }

    private List<long> BuildBranchSeqsLocked(string branchName, HashSet<string> visited)
    {
        if (!_branches.TryGetValue(branchName, out var info)) return new List<long>();
        if (!visited.Add(branchName)) return new List<long>();
        var seqs = new List<long>();
        if (info.Parent is not null && _branches.ContainsKey(info.Parent))
        {
            seqs.AddRange(BuildBranchSeqsLocked(info.Parent, visited).Where(s => s <= info.ForkSeq));
        }
        var own = _transitionBranches
          .Where(p => p.Value == branchName && p.Key <= info.Head)
          .Select(p => p.Key)
          .OrderBy(s => s);
        seqs.AddRange(own);
        return seqs;
    }

    private Transition<uint>? FindTransition(long sequence)
    {
        foreach (var t in _transactions.Log)
        {
            if (t.Sequence == sequence) return t;
        }
        return null;
    }

    // -- Persistence helpers ----------------------------------------------

    private void EnsureDefaultBranch()
    {
        lock (_lock)
        {
            var existing = _transactions.LastLoggedSequence;
            if (!_branches.ContainsKey(DefaultBranchName))
            {
                // Pre-existing transitions are attributed to the default branch.
                for (var s = 1L; s <= existing; s++)
                {
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
        var link = _branchesStore.CreateAndUpdate(_branchesStore.Constants.Null, _branchesStore.Constants.Null);
        _branchesStore.SetName(link, name);
    }

    private static string EncodeBranchMarker(BranchInfo info)
    {
        var parent = info.Parent ?? string.Empty;
        return string.Concat(
          BranchPrefix,
          info.Name,
          ":parent=", parent,
          ":fork=", info.ForkSeq.ToString(CultureInfo.InvariantCulture),
          ":head=", info.Head.ToString(CultureInfo.InvariantCulture));
    }

    private static bool TryDecodeBranchMarker(string text, out BranchInfo info)
    {
        info = default!;
        if (!text.StartsWith(BranchPrefix, StringComparison.Ordinal)) return false;
        var rest = text.Substring(BranchPrefix.Length);
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
            _branches.Clear();
            _tags.Clear();
            _transitionBranches.Clear();
            _branchLinks.Clear();
            _tagLinks.Clear();
            _currentBranch = DefaultBranchName;
            _currentBranchLink = 0;
            _appliedLink = 0;
            _currentApplied = 0;

            var any = _branchesStore.Constants.Any;
            var anyLink = new DoubletLink(any, any, any);
            foreach (var raw in _branchesStore.All(anyLink))
            {
                var link = new DoubletLink(raw);
                var name = _branchesStore.GetName(link.Index);
                if (string.IsNullOrEmpty(name)) continue;

                if (name.StartsWith(BranchPrefix, StringComparison.Ordinal))
                {
                    if (TryDecodeBranchMarker(name, out var info))
                    {
                        _branches[info.Name] = info;
                        _branchLinks[info.Name] = link.Index;
                    }
                }
                else if (name.StartsWith(CurrentPrefix, StringComparison.Ordinal))
                {
                    _currentBranch = name.Substring(CurrentPrefix.Length);
                    _currentBranchLink = link.Index;
                }
                else if (name.StartsWith(AppliedPrefix, StringComparison.Ordinal))
                {
                    var rest = name.Substring(AppliedPrefix.Length);
                    if (long.TryParse(rest, NumberStyles.Integer, CultureInfo.InvariantCulture, out var seq))
                    {
                        _currentApplied = seq;
                        _appliedLink = link.Index;
                    }
                }
                else if (name.StartsWith(TagPrefix, StringComparison.Ordinal))
                {
                    var rest = name.Substring(TagPrefix.Length);
                    var eq = rest.IndexOf('=');
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
                        var branchName = rest.Substring(colon + ":branch=".Length);
                        _transitionBranches[seq] = branchName;
                    }
                }
            }
        }
    }

    // -- INamedTypes forwarding -------------------------------------------

    public virtual string? GetName(uint link) => _transactions.GetName(link);
    public virtual uint SetName(uint link, string name) => _transactions.SetName(link, name);
    public virtual uint GetByName(string name) => _transactions.GetByName(name);
    public virtual void RemoveName(uint link) => _transactions.RemoveName(link);

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
        if (_trace) Console.WriteLine($"[VersionControl] {message}");
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
            if (!Inner.IsCommitted && !Inner.IsRolledBack)
            {
                _owner.RollbackVersionTransaction(this);
            }
        }
    }
}
