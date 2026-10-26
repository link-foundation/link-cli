using System.Collections;
using System.Collections.Immutable;
using System.Collections.ObjectModel;
using System.Numerics;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Private mutable candidate. Shared updates replace an immutable root;
/// freezing transfers a flat candidate without copying it a second time.</summary>
internal sealed class EngineRows<T> : IReadOnlyDictionary<T, EngineRow<T>>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    private readonly SortedDictionary<T, EngineRow<T>>? _flat;
    private ImmutableSortedDictionary<T, EngineRow<T>>? _shared;
    private SortedRunRows<T>? _runs;
    private bool _frozen;
    internal EngineRows(StateStructure structure)
    {
        if (structure == StateStructure.Shared) _shared = ImmutableSortedDictionary<T, EngineRow<T>>.Empty;
        else if (structure == StateStructure.SortedRuns) _runs = new();
        else _flat = new();
    }
    internal EngineRows(IReadOnlyDictionary<T, EngineRow<T>> rows, StateStructure structure,
        IReadOnlyDictionary<T, EngineRow<T>>? previous = null) : this(structure)
    {
        if (_flat is not null) foreach (var (id, row) in rows) _flat.Add(id, row);
        else if (_runs is not null)
        {
            if (rows is SortedRunRows<T> runs) _runs = runs;
            else
            {
                if (previous is SortedRunRows<T> old) _runs = old;
                foreach (var id in _runs.Keys) if (!rows.ContainsKey(id)) _runs = _runs.With(id, null);
                foreach (var (id, row) in rows) _runs = _runs.With(id, row);
            }
        }
        else if (rows is ImmutableSortedDictionary<T, EngineRow<T>> shared) _shared = shared;
        else
        {
            // Complete journal decoding is still O(N), but retained revisions
            // reuse unchanged entries rather than retaining N fresh nodes each.
            if (previous is ImmutableSortedDictionary<T, EngineRow<T>> old) _shared = old;
            foreach (var id in _shared!.Keys) if (!rows.ContainsKey(id)) _shared = _shared.Remove(id);
            foreach (var (id, row) in rows)
                if (!_shared.TryGetValue(id, out var present) || present != row) _shared = _shared.SetItem(id, row);
        }
    }
    private IReadOnlyDictionary<T, EngineRow<T>> Data => _flat is not null ? _flat : _shared is not null ? _shared : _runs!;
    public EngineRow<T> this[T key] => Data[key];
    public IEnumerable<T> Keys => Data.Keys;
    public IEnumerable<EngineRow<T>> Values => Data.Values;
    public int Count => Data.Count;
    public bool ContainsKey(T key) => Data.ContainsKey(key);
    public bool TryGetValue(T key, out EngineRow<T> value) => Data.TryGetValue(key, out value);
    public IEnumerator<KeyValuePair<T, EngineRow<T>>> GetEnumerator() => Data.GetEnumerator();
    IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
    internal void Set(T id, EngineRow<T> row)
    {
        if (_frozen) throw new InvalidOperationException("Candidate is already frozen.");
        if (_flat is not null) _flat[id] = row;
        else if (_runs is not null) _runs = _runs.With(id, row);
        else _shared = _shared!.SetItem(id, row);
    }
    internal void Remove(T id)
    {
        if (_frozen) throw new InvalidOperationException("Candidate is already frozen.");
        if (_flat is not null) _flat.Remove(id);
        else if (_runs is not null) _runs = _runs.With(id, null);
        else _shared = _shared!.Remove(id);
    }
    internal IReadOnlyDictionary<T, EngineRow<T>> Freeze()
    {
        _frozen = true;
        return _flat is not null ? new ReadOnlyDictionary<T, EngineRow<T>>(_flat) : Data;
    }
}
