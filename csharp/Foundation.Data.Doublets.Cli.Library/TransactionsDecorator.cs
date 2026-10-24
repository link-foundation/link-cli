using System.Collections.Concurrent;
using System.Globalization;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;
using Platform.Delegates;
using System.Numerics;

namespace Foundation.Data.Doublets.Cli;

/// <summary>The kind of write operation recorded by a transition.</summary>
public enum TransitionKind
{
    Create,
    Update,
    Delete
}

/// <summary>
/// Sync flushes data-store side-effects before <c>Commit</c> returns.
/// Async durably persists the transitions then applies the data-store
/// side-effects on a background thread (already-applied side-effects are
/// the common case for in-process inner stores).
/// </summary>
public enum CommitMode
{
    Sync,
    Async
}

/// <summary>
/// Retention policy for the transitions log:
/// <list type="bullet">
/// <item><see cref="Infinite"/> keeps every transition forever.</item>
/// <item><see cref="Chunked"/> processes the oldest <c>ChunkSize</c> records
/// once the live log reaches that size, archiving committed changes in
/// <c>ArchiveDirectory</c> and discarding aborted records.</item>
/// <item><see cref="Sized"/> drops the oldest transitions once the live log
/// exceeds <c>MaxTransitions</c>, after verifying each is applied or rolled
/// back. Pending writes remain in the live log.</item>
/// </list>
/// </summary>
public abstract record LogRetentionPolicy
{
    public sealed record Infinite() : LogRetentionPolicy;
    public sealed record Chunked(long ChunkSize, string ArchiveDirectory) : LogRetentionPolicy;
    public sealed record Sized(long MaxTransitions) : LogRetentionPolicy;

    public static LogRetentionPolicy Default { get; } = new Infinite();

    /// <summary>
    /// Parses a CLI spec: <c>infinite</c>, <c>sized:&lt;n&gt;</c>, or
    /// <c>chunked:&lt;n&gt;:&lt;dir&gt;</c>.
    /// </summary>
    public static LogRetentionPolicy Parse(string spec)
    {
        ArgumentNullException.ThrowIfNull(spec);
        var trimmed = spec.Trim();
        if (trimmed.Length == 0 || trimmed.Equals("infinite", StringComparison.OrdinalIgnoreCase))
        {
            return new Infinite();
        }

        var lowered = trimmed.ToLowerInvariant();
        if (lowered.StartsWith("sized:", StringComparison.Ordinal))
        {
            var rest = trimmed.Substring("sized:".Length);
            if (!long.TryParse(rest, NumberStyles.Integer, CultureInfo.InvariantCulture, out var max) || max < 0)
            {
                throw new ArgumentException($"Invalid sized retention spec '{spec}'.", nameof(spec));
            }
            return new Sized(max);
        }

        if (lowered.StartsWith("chunked:", StringComparison.Ordinal))
        {
            var rest = trimmed.Substring("chunked:".Length);
            var colon = rest.IndexOf(':');
            if (colon <= 0 || colon == rest.Length - 1)
            {
                throw new ArgumentException($"Invalid chunked retention spec '{spec}'.", nameof(spec));
            }
            var sizeText = rest.Substring(0, colon);
            var dir = rest.Substring(colon + 1);
            if (!long.TryParse(sizeText, NumberStyles.Integer, CultureInfo.InvariantCulture, out var chunkSize) || chunkSize <= 0)
            {
                throw new ArgumentException($"Invalid chunked size in '{spec}'.", nameof(spec));
            }
            return new Chunked(chunkSize, dir);
        }

        throw new ArgumentException($"Unknown retention spec '{spec}'.", nameof(spec));
    }
}

/// <summary>
/// A reversible write captured by the transactions layer. Holds both
/// <see cref="Before"/> and <see cref="After"/> link states so the
/// operation can be undone (replay <c>After → Before</c>) or replayed
/// (<c>Before → After</c>).
/// </summary>
public readonly record struct Transition<TLinkAddress>(
  Guid TransactionId,
  long Sequence,
  DateTimeOffset Timestamp,
  TransitionKind Kind,
  Link<TLinkAddress> Before,
  Link<TLinkAddress> After)
  where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    internal const string SchemaVersion = "v1";

    /// <summary>Encodes the transition as a single line stored as the
    /// <em>name</em> of one link in the log doublets store.</summary>
    public string Serialize()
    {
        return string.Join('|',
          SchemaVersion,
          TransactionId.ToString("N"),
          Sequence.ToString(CultureInfo.InvariantCulture),
          Timestamp.ToUnixTimeMilliseconds().ToString(CultureInfo.InvariantCulture),
          ((int)Kind).ToString(CultureInfo.InvariantCulture),
          FormatLink(Before),
          FormatLink(After));
    }

    /// <summary>Formats a link as <c>index,source,target</c> in decimal,
    /// so that a log written by a <c>uint</c>-addressed store reads back
    /// unchanged in a <c>ulong</c>-addressed one.</summary>
    private static string FormatLink(Link<TLinkAddress> link)
    {
        return string.Concat(
          Format(link.Index), ",", Format(link.Source), ",", Format(link.Target));
    }

    private static string Format(TLinkAddress address)
    {
        return address.ToString(null, CultureInfo.InvariantCulture);
    }

    public static bool TryParse(string text, out Transition<TLinkAddress> transition)
    {
        transition = default;
        if (string.IsNullOrWhiteSpace(text)) return false;
        var parts = text.Split('|');
        if (parts.Length < 7 || parts[0] != SchemaVersion) return false;
        if (!Guid.TryParseExact(parts[1], "N", out var txId)) return false;
        if (!long.TryParse(parts[2], NumberStyles.Integer, CultureInfo.InvariantCulture, out var seq)) return false;
        if (!long.TryParse(parts[3], NumberStyles.Integer, CultureInfo.InvariantCulture, out var ms)) return false;
        if (!int.TryParse(parts[4], NumberStyles.Integer, CultureInfo.InvariantCulture, out var kindValue)
            || !Enum.IsDefined((TransitionKind)kindValue)) return false;
        if (ms < DateTimeOffset.MinValue.ToUnixTimeMilliseconds()
            || ms > DateTimeOffset.MaxValue.ToUnixTimeMilliseconds()) return false;
        if (!TryParseLink(parts[5], out var before)) return false;
        if (!TryParseLink(parts[6], out var after)) return false;
        transition = new Transition<TLinkAddress>(
          txId,
          seq,
          DateTimeOffset.FromUnixTimeMilliseconds(ms),
          (TransitionKind)kindValue,
          before,
          after);
        return true;
    }

    private static bool TryParseLink(string text, out Link<TLinkAddress> link)
    {
        link = default;
        var parts = text.Split(',');
        if (parts.Length != 3) return false;
        if (!TLinkAddress.TryParse(parts[0], NumberStyles.Integer, CultureInfo.InvariantCulture, out var index)) return false;
        if (!TLinkAddress.TryParse(parts[1], NumberStyles.Integer, CultureInfo.InvariantCulture, out var source)) return false;
        if (!TLinkAddress.TryParse(parts[2], NumberStyles.Integer, CultureInfo.InvariantCulture, out var target)) return false;
        link = new Link<TLinkAddress>(index, source, target);
        return true;
    }
}

/// <summary>A live transaction handle. Disposal without commit rolls
/// back automatically (R10).</summary>
public interface ITransaction<TLinkAddress> : IDisposable
  where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    Guid Id { get; }
    DateTimeOffset StartedAt { get; }
    /// <summary>A durable commit decision exists. If checkpoint persistence
    /// failed, retry Commit, Rollback or Dispose to finish that decision.</summary>
    bool IsCommitted { get; }
    bool IsRolledBack { get; }
    IReadOnlyList<Transition<TLinkAddress>> Transitions { get; }
    void Commit();
    void Rollback();
    Task CommitAsync(CancellationToken cancellationToken = default);
}

/// <summary>A links store with transactional semantics layered on top
/// of the underlying <see cref="INamedTypesLinks{TLinkAddress}"/>.</summary>
public interface ITransactionsLinks<TLinkAddress> : INamedTypesLinks<TLinkAddress>
  where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    ITransaction<TLinkAddress> BeginTransaction();
    Task<ITransaction<TLinkAddress>> BeginTransactionAsync(CancellationToken cancellationToken = default);
    IReadOnlyList<Transition<TLinkAddress>> Log { get; }
    LogRetentionPolicy RetentionPolicy { get; set; }
    CommitMode CommitMode { get; set; }
    void Recover();
    long AppliedSequence { get; }
    long LastLoggedSequence { get; }
}

/// <summary>
/// Decorator that records every <c>Create</c>/<c>Update</c>/<c>Delete</c>
/// as a reversible <see cref="Transition{TLinkAddress}"/> in a sidecar doublets log
/// store. Supports explicit transactions, sync/async commits, three log
/// retention policies, and crash recovery. Optional — no behavioural
/// change if not opted in (R8).
/// </summary>
public partial class TransactionsDecorator<TLinkAddress> : LinksDecoratorBase<TLinkAddress>, ITransactionsLinks<TLinkAddress>, IDisposable
  where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    internal const string CommitMarkerPrefix = "__transactions:commit:";
    internal const string RollbackMarkerPrefix = "__transactions:rollback:";
    internal const string AppliedMarkerPrefix = "__transactions:applied:";
    internal const string TransitionNamePrefix = "__transactions:transition:";

    private readonly INamedTypesLinks<TLinkAddress> _inner;
    private readonly INamedTypesLinks<TLinkAddress> _logStore;
    private readonly bool _trace;
    private readonly object _lock = new();
    private readonly object _shutdownLock = new();
    private readonly AsyncLocal<BackgroundWorkContext?> _backgroundWork = new();
    private readonly List<Transition<TLinkAddress>> _log = new();
    private readonly HashSet<Guid> _committed = new();
    private readonly HashSet<Guid> _rolledBack = new();
    private readonly HashSet<long> _applied = new();
    private readonly BlockingCollection<Func<Task>> _asyncQueue = new();
    private readonly CancellationTokenSource _backgroundCts = new();
    // The base facade can retain this object if recovery fails construction.
    // Disposal must also be safe before a real worker has been started.
    private readonly Task _backgroundWorker = Task.CompletedTask;
    private readonly Action _flushData;
    private readonly Action _flushLog;

    /// <summary>True when both data and journal have explicit stable-storage flushes.</summary>
    public bool SupportsDurableCommit { get; }
    private Transaction? _current;
    private long _sequenceCounter;
    private long _appliedSequence;
    private bool _disposed;
    private bool _replaying;
    private LogRetentionPolicy _retentionPolicy;
    private CommitMode _commitMode;

    public TransactionsDecorator(
      INamedTypesLinks<TLinkAddress> inner,
      INamedTypesLinks<TLinkAddress> logStore,
      LogRetentionPolicy? retentionPolicy = null,
      CommitMode commitMode = CommitMode.Sync,
      bool trace = false,
      Action? flushData = null,
      Action? flushLog = null)
      : base(inner)
    {
        _inner = inner;
        _logStore = logStore;
        _retentionPolicy = retentionPolicy ?? LogRetentionPolicy.Default;
        _commitMode = commitMode;
        _trace = trace;
        var durableData = inner as IDurableLinks;
        var durableLog = logStore as IDurableLinks;
        SupportsDurableCommit = (flushData is not null || durableData?.SupportsDurableFlush == true)
            && (flushLog is not null || durableLog?.SupportsDurableFlush == true);
        _flushData = flushData ?? (durableData?.SupportsDurableFlush == true ? durableData.Flush : static () => { });
        _flushLog = flushLog ?? (durableLog?.SupportsDurableFlush == true ? durableLog.Flush : static () => { });
        Recover();
        _backgroundWorker = Task.Run(RunBackgroundWorker);
    }

    public CommitMode CommitMode
    {
        get { lock (_lock) return _commitMode; }
        set { lock (_lock) _commitMode = value; }
    }

    public LogRetentionPolicy RetentionPolicy
    {
        get { lock (_lock) return _retentionPolicy; }
        set { lock (_lock) _retentionPolicy = value ?? LogRetentionPolicy.Default; }
    }

    public IReadOnlyList<Transition<TLinkAddress>> Log
    {
        get { lock (_lock) return _log.ToArray(); }
    }

    public long AppliedSequence { get { lock (_lock) return _appliedSequence; } }
    public long LastLoggedSequence { get { lock (_lock) return _sequenceCounter; } }

    public virtual ITransaction<TLinkAddress> BeginTransaction()
    {
        lock (_lock)
        {
            if (_current is not null)
            {
                throw new InvalidOperationException("Nested transactions are not supported.");
            }
            _current = new Transaction(this, autoCommit: false);
            Trace($"Began transaction {_current.Id:N}.");
            return _current;
        }
    }

    public virtual Task<ITransaction<TLinkAddress>> BeginTransactionAsync(CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(BeginTransaction());
    }

    /// <summary>Rolls back the current transaction, including retrying cleanup
    /// after an automatic write's rollback failed. A known durable commit instead
    /// finishes its checkpoint. A failure keeps either outcome pending.</summary>
    public virtual void Rollback()
    {
        lock (_lock)
        {
            if (_current is not null) OnRollback(_current);
        }
    }

    // Write API (wraps the user's handler so we observe before/after) -------

    public override TLinkAddress Create(IList<TLinkAddress>? substitution, WriteHandler<TLinkAddress>? handler)
    {
        return RunWrite(TransitionKind.Create, h => _inner.Create(substitution, h), handler);
    }

    public override TLinkAddress Update(IList<TLinkAddress>? restriction, IList<TLinkAddress>? substitution, WriteHandler<TLinkAddress>? handler)
    {
        return RunWrite(TransitionKind.Update, h => _inner.Update(restriction, substitution, h), handler);
    }

    public override TLinkAddress Delete(IList<TLinkAddress>? restriction, WriteHandler<TLinkAddress>? handler)
    {
        return RunWrite(TransitionKind.Delete, h => _inner.Delete(restriction, h), handler);
    }

    private TLinkAddress RunWrite(
      TransitionKind kind,
      Func<WriteHandler<TLinkAddress>, TLinkAddress> innerCall,
      WriteHandler<TLinkAddress>? userHandler)
    {
        if (_replaying)
        {
            return innerCall(userHandler ?? NullHandler);
        }

        Transaction transaction;
        bool ownsTransaction;
        lock (_lock)
        {
            if (_sequenceCounter == long.MaxValue)
                throw new InvalidOperationException("Transaction sequence space is exhausted.");
            if (_current is null)
            {
                _current = new Transaction(this, autoCommit: true);
                ownsTransaction = true;
            }
            else
            {
                EnsureWriteAllowedLocked();
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
            try
            {
                RecordTransitions(transaction, kind, observedOrder.Select(key => observed[key]));
            }
            catch
            {
                if (ownsTransaction) transaction.Rollback();
                throw;
            }
        }

        TLinkAddress result;
        try
        {
            result = innerCall(wrapped);
        }
        catch
        {
            // A callback describes a change already made, even when the
            // storage adapter or user handler subsequently throws.
            RecordObservedChanges();
            if (ownsTransaction)
            {
                if (observedOrder.Count > 0)
                {
                    transaction.Rollback();
                }
                else
                {
                    lock (_lock)
                    {
                        if (_current == transaction) _current = null;
                    }
                }
            }
            throw;
        }

        RecordObservedChanges();

        if (ownsTransaction)
        {
            transaction.Commit();
        }

        return result;
    }

    private static Link<TLinkAddress> LinkOrEmpty(IList<TLinkAddress>? raw)
    {
        return raw is null ? default : new Link<TLinkAddress>(raw);
    }

    private static TLinkAddress NullHandler(IList<TLinkAddress>? before, IList<TLinkAddress>? after) => TLinkAddress.Zero;

    // INamedTypes forwarding ------------------------------------------------

    public virtual string? GetName(TLinkAddress link) => _inner.GetName(link);
    public virtual TLinkAddress SetName(TLinkAddress link, string name)
    {
        lock (_lock)
        {
            EnsureWriteAllowedLocked();
            return _inner.SetName(link, name);
        }
    }
    public virtual TLinkAddress GetByName(string name) => _inner.GetByName(name);
    public virtual void RemoveName(TLinkAddress link)
    {
        lock (_lock)
        {
            EnsureWriteAllowedLocked();
            _inner.RemoveName(link);
        }
    }

    // Disposal --------------------------------------------------------------

    /// <summary>
    /// Stops the background worker. The wrapped data store and log store
    /// are not disposed here; callers are expected to own those.
    /// </summary>
    public void Dispose()
    {
        Dispose(true);
        GC.SuppressFinalize(this);
    }

    /// <summary>
    /// Stops the background worker. Derived decorators that own extra
    /// resources override this and call <c>base.Dispose(disposing)</c>.
    /// </summary>
    protected virtual void Dispose(bool disposing)
    {
        if (disposing)
        {
            Shutdown();
        }
    }

    /// <summary>
    /// Stops and joins the background worker before borrowed stores may close.
    /// Concurrent callers all wait for completion. Queued checkpoint work may
    /// be cancelled; committed journal records remain recoverable on reopen.
    /// Callers must finish foreground operations first and callbacks must return.
    /// </summary>
    /// <exception cref="InvalidOperationException">
    /// Called from active background work or while holding the storage lock.
    /// Such a caller cannot wait for worker completion without risking deadlock.
    /// </exception>
    public virtual void Shutdown()
    {
        // Check before the shutdown mutex: an external caller may already hold
        // it while waiting for this callback (or its Task.Run descendant).
        var work = _backgroundWork.Value;
        if ((work is not null && Volatile.Read(ref work.Active) != 0) || Monitor.IsEntered(_lock))
        {
            throw new InvalidOperationException("Shutdown must run outside active storage callbacks.");
        }
        lock (_shutdownLock)
        {
            if (_disposed) return;
            _asyncQueue.CompleteAdding();
            _backgroundCts.Cancel();
            // A timed join cannot establish that borrowed mappings are unused.
            // Never hold the storage lock while waiting for its current owner.
            _backgroundWorker.GetAwaiter().GetResult();
            _asyncQueue.Dispose();
            _backgroundCts.Dispose();
            _disposed = true;
        }
    }

    private void TryRevertTransition(Transition<TLinkAddress> transition)
    {
        try
        {
            if (transition.Before.Index == TLinkAddress.Zero)
            {
                DeleteIfExists(transition.After.Index);
            }
            else
            {
                RestoreLink(transition.Before);
            }
        }
        catch (Exception ex)
        {
            Trace($"Failed to revert transition seq={transition.Sequence}: {ex.Message}");
            throw;
        }
    }

    /// <summary>
    /// Revert a single transition's side-effect against the data store
    /// without writing a new log entry. Intended for use by higher-level
    /// decorators (e.g. version control) that need to drive replay/rewind
    /// without producing additional transitions.
    /// </summary>
    public virtual void RevertTransition(Transition<TLinkAddress> transition)
    {
        lock (_lock)
        {
            _replaying = true;
            try
            {
                TryRevertTransition(transition);
            }
            finally
            {
                _replaying = false;
            }
        }
    }

    /// <summary>
    /// Apply a single transition's side-effect against the data store
    /// without writing a new log entry. Intended for use by higher-level
    /// decorators (e.g. version control) that need to drive replay/rewind
    /// without producing additional transitions.
    /// </summary>
    public virtual void ApplyTransition(Transition<TLinkAddress> transition)
    {
        lock (_lock)
        {
            _replaying = true;
            try
            {
                TryApplyTransition(transition, recordApplied: false);
            }
            finally
            {
                _replaying = false;
            }
        }
    }

    private void TryApplyTransition(Transition<TLinkAddress> transition, bool recordApplied)
    {
        try
        {
            if (transition.After.Index == TLinkAddress.Zero)
            {
                DeleteIfExists(transition.Before.Index);
            }
            else
            {
                RestoreLink(transition.After);
            }

            if (recordApplied)
            {
                _flushData();
                MarkApplied(transition);
                _flushLog();
            }
        }
        catch (Exception ex)
        {
            Trace($"Failed to apply transition seq={transition.Sequence}: {ex.Message}");
            throw;
        }
    }

    private void MarkApplied(Transition<TLinkAddress> transition)
    {
        if (!_applied.Contains(transition.Sequence))
        {
            WriteMarker(AppliedMarkerPrefix + transition.Sequence.ToString(CultureInfo.InvariantCulture));
            _applied.Add(transition.Sequence);
            if (transition.Sequence > _appliedSequence) _appliedSequence = transition.Sequence;
        }
    }

    private void RestoreLink(Link<TLinkAddress> link)
    {
        if (link.Index == TLinkAddress.Zero) return;
        if (!_inner.Exists(link.Index))
        {
            _inner.EnsureCreated(link.Index);
        }
        _inner.Update(
          new Link<TLinkAddress>(link.Index, _inner.Constants.Any, _inner.Constants.Any),
          new Link<TLinkAddress>(link.Index, link.Source, link.Target),
          null);
    }

    private void DeleteIfExists(TLinkAddress index)
    {
        if (index != TLinkAddress.Zero && _inner.Exists(index))
        {
            _inner.Delete(new Link<TLinkAddress>(index, _inner.Constants.Any, _inner.Constants.Any), null);
        }
    }

    internal void WriteTransitionToLog(Transition<TLinkAddress> transition)
    {
        var link = _logStore.CreateAndUpdate(_logStore.Constants.Null, _logStore.Constants.Null);
        var name = TransitionNamePrefix + transition.Serialize();
        _logStore.SetName(link, name);
    }

    internal void WriteMarker(string name)
    {
        var link = _logStore.CreateAndUpdate(_logStore.Constants.Null, _logStore.Constants.Null);
        _logStore.SetName(link, name);
    }

    private static void InsertOrdered(List<Transition<TLinkAddress>> list, Transition<TLinkAddress> transition)
    {
        var lo = 0;
        var hi = list.Count;
        while (lo < hi)
        {
            var mid = (lo + hi) >> 1;
            if (list[mid].Sequence < transition.Sequence) lo = mid + 1; else hi = mid;
        }
        list.Insert(lo, transition);
    }

    private void EnforceRetentionLocked()
    {
        switch (_retentionPolicy)
        {
            case LogRetentionPolicy.Infinite:
                return;
            case LogRetentionPolicy.Sized sized:
                EnforceSizedLocked(sized.MaxTransitions);
                break;
            case LogRetentionPolicy.Chunked chunked:
                EnforceChunkedLocked(chunked);
                break;
        }
    }

    private void EnforceSizedLocked(long maxTransitions)
    {
        if (maxTransitions <= 0) return;
        while (_log.Count > maxTransitions)
        {
            var head = _log[0];
            if (!_rolledBack.Contains(head.TransactionId) && !_applied.Contains(head.Sequence))
            {
                if (!_committed.Contains(head.TransactionId)) break;
                // R7: never drop an un-applied transition.
                TryApplyTransition(head, recordApplied: true);
                if (!_applied.Contains(head.Sequence)) break;
            }
            _log.RemoveAt(0);
            Trace($"Dropped completed transition seq={head.Sequence} per sized retention.");
        }
    }

    private void EnforceChunkedLocked(LogRetentionPolicy.Chunked chunked)
    {
        if (chunked.ChunkSize <= 0) return;
        if (_log.Count < chunked.ChunkSize) return;

        var chunk = _log.Take((int)chunked.ChunkSize).ToList();
        // A durable rollback makes its records reclaimable without reapplying
        // their writes. Archive only committed changes, not aborted operations.
        var archived = chunk.Where(t => !_rolledBack.Contains(t.TransactionId)).ToList();
        foreach (var transition in archived)
        {
            if (!_applied.Contains(transition.Sequence))
            {
                if (!_committed.Contains(transition.TransactionId)) return;
                TryApplyTransition(transition, recordApplied: true);
                if (!_applied.Contains(transition.Sequence)) return; // never drop unapplied
            }
        }

        if (archived.Count == 0)
        {
            _log.RemoveRange(0, chunk.Count);
            return;
        }

        try
        {
            Directory.CreateDirectory(chunked.ArchiveDirectory);
            var fileName = Path.Combine(
              chunked.ArchiveDirectory,
              $"transitions-chunk-{DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()}-{Guid.NewGuid():N}.log");
            using (var writer = new StreamWriter(fileName, append: false))
            {
                foreach (var t in archived) writer.WriteLine(t.Serialize());
            }
            Trace($"Archived {archived.Count} transitions to {fileName}.");
        }
        catch (Exception ex)
        {
            Trace($"Chunk archive failed: {ex.Message}");
            return;
        }

        _log.RemoveRange(0, chunk.Count);
    }

    private async Task ApplyTransitionsAsync(IReadOnlyList<Transition<TLinkAddress>> transitions)
    {
        foreach (var transition in transitions)
        {
            try
            {
                lock (_lock)
                {
                    // Side-effects normally already applied (inner store ran
                    // them inline). Re-apply only if needed and mark applied.
                    _flushData();
                    MarkApplied(transition);
                    _flushLog();
                }
            }
            catch (Exception error)
            {
                Trace($"Asynchronous checkpoint failed; recovery will resume on reopen: {error.Message}");
            }
        }

        lock (_lock)
        {
            EnforceRetentionLocked();
        }
        await Task.CompletedTask;
    }

    private sealed class BackgroundWorkContext
    {
        // Descendant tasks can retain the ExecutionContext after work finishes.
        // Shared state prevents a stale AsyncLocal value from rejecting them.
        public int Active = 1;
    }

    private void RunBackgroundWorker()
    {
        try
        {
            foreach (var work in _asyncQueue.GetConsumingEnumerable(_backgroundCts.Token))
            {
                var context = new BackgroundWorkContext();
                _backgroundWork.Value = context;
                try { work().GetAwaiter().GetResult(); }
                catch (Exception error) { Trace($"Background checkpoint failed: {error.Message}"); }
                finally
                {
                    Volatile.Write(ref context.Active, 0);
                    _backgroundWork.Value = null;
                }
            }
        }
        catch (OperationCanceledException) { /* expected */ }
        catch { /* background should never blow up */ }
    }

    private void Trace(string message)
    {
        if (_trace) DiagnosticOutput.WriteLine($"[Transactions] {message}");
    }

    /// <summary>Conventional sidecar filename for the transitions log.</summary>
    public static string MakeTransitionsDatabaseFilename(string databaseFilename)
    {
        ArgumentNullException.ThrowIfNull(databaseFilename);
        var filenameWithoutExtension = Path.GetFileNameWithoutExtension(databaseFilename);
        var directory = Path.GetDirectoryName(databaseFilename);
        return Path.Combine(directory ?? string.Empty, $"{filenameWithoutExtension}.transitions.links");
    }

    // Transaction handle ----------------------------------------------------

    internal sealed class Transaction : ITransaction<TLinkAddress>
    {
        private readonly TransactionsDecorator<TLinkAddress> _owner;
        private readonly List<Transition<TLinkAddress>> _transitions = new();
        private readonly bool _autoCommit;
        private int _state; // 0 = open, 1 = committed, 2 = rolled back
        public bool IsRollbackOnly { get; private set; }

        public Transaction(TransactionsDecorator<TLinkAddress> owner, bool autoCommit)
        {
            _owner = owner;
            _autoCommit = autoCommit;
            Id = Guid.NewGuid();
            StartedAt = DateTimeOffset.UtcNow;
        }

        public Guid Id { get; }
        public DateTimeOffset StartedAt { get; }
        public bool IsCommitted => _state == 1;
        public bool IsRolledBack => _state == 2;
        public IReadOnlyList<Transition<TLinkAddress>> Transitions => _transitions;

        internal void AddTransition(Transition<TLinkAddress> transition) => _transitions.Add(transition);
        internal void MarkCommitted() => _state = 1;
        internal void MarkRolledBack() => _state = 2;
        internal void MarkRollbackOnly() => IsRollbackOnly = true;

        public void Commit() => _owner.OnCommit(this, forceAsync: false);

        public Task CommitAsync(CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            _owner.OnCommit(this, forceAsync: true);
            return Task.CompletedTask;
        }

        public void Rollback() => _owner.OnRollback(this);

        public void Dispose()
        {
            if (IsCommitted)
            {
                _owner.OnCommit(this, forceAsync: false);
            }
            else if (_state == 0)
            {
                // Per-write auto transactions should not auto-rollback if the
                // caller forgot to commit (Commit happens automatically in
                // RunWrite); for explicit user transactions, dispose = rollback.
                if (_autoCommit)
                {
                    _owner.OnCommit(this, forceAsync: false);
                }
                else
                {
                    _owner.OnRollback(this);
                }
            }
        }
    }
}

/// <summary>
/// The <c>uint</c>-addressed transactions decorator used by the
/// <c>clink</c> CLI.
/// </summary>
/// <remarks>
/// Nothing but a convenience name for
/// <see cref="TransactionsDecorator{TLinkAddress}"/> closed over
/// <see cref="uint"/>: consumers that address their doublets store with
/// <see cref="ulong"/> (or any other <see cref="IUnsignedNumber{TSelf}"/>)
/// use the generic form directly.
/// </remarks>
public class TransactionsDecorator : TransactionsDecorator<uint>
{
    /// <inheritdoc cref="TransactionsDecorator{TLinkAddress}(INamedTypesLinks{TLinkAddress}, INamedTypesLinks{TLinkAddress}, LogRetentionPolicy, CommitMode, bool, Action, Action)"/>
    public TransactionsDecorator(
      INamedTypesLinks<uint> inner,
      INamedTypesLinks<uint> logStore,
      LogRetentionPolicy? retentionPolicy = null,
      CommitMode commitMode = CommitMode.Sync,
      bool trace = false,
      Action? flushData = null,
      Action? flushLog = null)
      : base(inner, logStore, retentionPolicy, commitMode, trace, flushData, flushLog)
    {
    }
}
