using System.Collections.ObjectModel;
using System.Collections.Immutable;
using System.Numerics;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Engine;

public enum ConcurrencyControl { Optimistic, Exclusive, Pessimistic }
public enum IsolationLevel { ReadCommitted, Snapshot, Serializable }
public enum SerializableValidation { Revision, ReadSet }
public enum EngineConstraint { References, UniquePairs, UniqueNames }
/// <summary>Independent in-memory representation; journal format is unchanged.</summary>
public enum StateStructure { Flat, Shared }

public sealed record EngineOptions
{
    public StateStructure StateStructure { get; init; } = StateStructure.Flat;
    public ConcurrencyControl Concurrency { get; init; } = ConcurrencyControl.Optimistic;
    public IsolationLevel Isolation { get; init; } = IsolationLevel.Serializable;
    public SerializableValidation SerializableValidation { get; init; } = SerializableValidation.Revision;
    /// <summary>Zero disables history, null retains all, a positive limit includes current.</summary>
    public int? HistoryLimit { get; init; } = 0;
    public IReadOnlyList<EngineConstraint> Constraints { get; init; } = Array.Empty<EngineConstraint>();
    public bool RequireDurableFlush { get; init; }
    public Action<string>? Trace { get; init; }
}

public readonly record struct EngineRow<T>(Link<T> Link, string? Name)
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>;

public sealed class EngineSnapshot<T>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    public ulong Revision { get; }
    public IReadOnlyDictionary<T, EngineRow<T>> Rows { get; }
    internal EngineSnapshot(ulong revision, SortedDictionary<T, EngineRow<T>> rows)
    { Revision = revision; Rows = new ReadOnlyDictionary<T, EngineRow<T>>(rows); }
    private EngineSnapshot(ulong revision, EngineRows<T> rows)
    { Revision = revision; Rows = rows.Freeze(); }
    internal static EngineSnapshot<T> Create(ulong revision, EngineRows<T> rows) => new(revision, rows);
    internal EngineSnapshot<T> Adapt(StateStructure structure, EngineSnapshot<T>? previous = null)
        => (Rows is ImmutableSortedDictionary<T, EngineRow<T>>) == (structure == StateStructure.Shared)
            ? this : Create(Revision, new(Rows, structure, previous?.Rows));
}

/// <summary>Optional isolated engine. The revision log is authoritative; all writes
/// must use sessions of this engine. Sessions themselves are single-threaded.</summary>
public sealed partial class LinksEngine<T>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    internal readonly object Gate = new();
    internal readonly EngineOptions Options;
    internal EngineSnapshot<T> Current;
    internal Dictionary<T, ulong>? Changed;
    internal Dictionary<T, object>? WriteOwners;
    internal BigInteger Next = BigInteger.One;
    internal BigInteger CommittedNext = BigInteger.One;
    internal int Active;
    internal bool Poisoned;
    private readonly ICommitLog _log;
    private readonly SortedDictionary<ulong, EngineSnapshot<T>> _history = new();

    public LinksEngine(ICommitLog log, EngineOptions? options = null)
    {
        _log = log;
        Options = options ?? new();
        if (Options.HistoryLimit < 0 || !Enum.IsDefined(Options.Isolation) || !Enum.IsDefined(Options.Concurrency)
            || !Enum.IsDefined(Options.SerializableValidation)
            || !Enum.IsDefined(Options.StateStructure)
            || (Options.SerializableValidation == SerializableValidation.ReadSet && Options.Isolation != IsolationLevel.Serializable)
            || Options.Constraints.Any(c => !Enum.IsDefined(c)))
            throw new ArgumentException("Invalid engine strategy.", nameof(options));
        if (Options.RequireDurableFlush && !log.SupportsDurableFlush)
            throw new ArgumentException("Selected log does not support durable flush.", nameof(log));
        // Freeze caller-owned collections before using them as policy.
        Options = Options with { Constraints = Options.Constraints.ToArray() };
        Current = EngineSnapshot<T>.Create(0, new(Options.StateStructure));
        Retain();
        foreach (var record in log.Read())
        {
            var (snapshot, next) = RevisionJournal<T>.Decode(record);
            if (snapshot.Revision != checked(Current.Revision + 1) || next < Next)
                throw new InvalidDataException("Journal revision or allocator moved backwards.");
            Validate(snapshot.Rows);
            Current = snapshot.Adapt(Options.StateStructure, Current);
            Next = next;
            CommittedNext = next;
            Retain();
        }
        // No session predates open; recovered rows need no conflict generations.
    }

    internal void CheckReady()
    { if (Poisoned) throw new InvalidOperationException("Indeterminate journal outcome; close and reopen the engine."); }

    public EngineSession<T> Begin()
    {
        lock (Gate)
        {
            CheckReady();
            if (Options.Concurrency == ConcurrencyControl.Exclusive && Active > 0)
                throw new InvalidOperationException("Busy: an exclusive transaction is active.");
            Active++;
            return new(this, Current);
        }
    }
    public EngineSnapshot<T> Snapshot()
    { lock (Gate) { CheckReady(); return Current; } }
    public EngineSnapshot<T> SnapshotAt(ulong revision)
    {
        lock (Gate)
        {
            CheckReady();
            return revision == Current.Revision ? Current : _history.TryGetValue(revision, out var snapshot)
                ? snapshot : throw new KeyNotFoundException("Revision is not retained.");
        }
    }
    public TResult WithLog<TResult>(Func<ICommitLog, TResult> operation)
    {
        lock (Gate)
        {
            CheckReady();
            try { return operation(_log); }
            catch { Poisoned = true; throw; }
        }
    }
    internal BigInteger NextAfter(BigInteger id)
    {
        var next = BigInteger.Max(Next, id + 1);
        if (next > BigInteger.CreateChecked(UInt128.MaxValue)) throw new OverflowException("Address space exhausted.");
        return next;
    }
    internal void Publish(EngineRows<T> rows, IEnumerable<T> writes)
    {
        Validate(rows);
        var snapshot = EngineSnapshot<T>.Create(checked(Current.Revision + 1), rows);
        var record = RevisionJournal<T>.Encode(snapshot, Next);
        try { _log.Append(record); _log.Flush(); }
        catch (Exception error)
        {
            Poisoned = true;
            throw new IOException("Indeterminate commit; reopen the engine to discover the outcome.", error);
        }
        if (Active > 0 && (Options.Concurrency == ConcurrencyControl.Pessimistic
            || (Options.Concurrency == ConcurrencyControl.Optimistic && (Options.Isolation != IsolationLevel.Serializable || Options.SerializableValidation == SerializableValidation.ReadSet))))
            foreach (var id in writes) (Changed ??= new())[id] = snapshot.Revision;
        Current = snapshot;
        CommittedNext = Next;
        Retain();
        EngineDiagnostics.Trace(Options.Trace, $"Committed revision={snapshot.Revision}");
    }
    private void Retain()
    {
        if (Options.HistoryLimit == 0) return;
        _history[Current.Revision] = Current;
        while (Options.HistoryLimit is { } limit && _history.Count > limit) _history.Remove(_history.First().Key);
    }
    internal void Validate(IReadOnlyDictionary<T, EngineRow<T>> rows)
    {
        foreach (var constraint in Options.Constraints)
        {
            switch (constraint)
            {
                case EngineConstraint.References:
                    if (rows.Values.Any(row => (row.Link.Source != T.Zero && !rows.ContainsKey(row.Link.Source))
                        || (row.Link.Target != T.Zero && !rows.ContainsKey(row.Link.Target))))
                        throw new InvalidOperationException("Constraint: dangling link reference.");
                    break;
                case EngineConstraint.UniquePairs:
                    if (rows.Values.Select(row => (row.Link.Source, row.Link.Target)).Distinct().Count() != rows.Count)
                        throw new InvalidOperationException("Constraint: duplicate pair.");
                    break;
                case EngineConstraint.UniqueNames:
                    var names = rows.Values.Select(row => row.Name).OfType<string>().ToArray();
                    if (names.Distinct(StringComparer.Ordinal).Count() != names.Length)
                        throw new InvalidOperationException("Constraint: duplicate name.");
                    break;
            }
        }
    }
}

internal static class EngineDiagnostics
{
    // Diagnostics must not turn an acknowledged commit into a reported failure.
    internal static void Trace(Action<string>? trace, string message)
    { try { trace?.Invoke(message); } catch { /* The storage result remains authoritative. */ } }
}
