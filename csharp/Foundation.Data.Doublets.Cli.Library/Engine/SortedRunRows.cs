using System.Collections;
using System.Numerics;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Selected immutable runs. Lower event-count tiers contain newer edits.
/// Ordered scans have a lazy, per-view materialization cost.</summary>
internal sealed class SortedRunRows<T> : IReadOnlyDictionary<T, EngineRow<T>>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    private readonly record struct Entry(T Key, EngineRow<T>? Value);
    private readonly Entry[]?[] _levels;
    private readonly Lazy<SortedDictionary<T, EngineRow<T>>> _ordered;
    public int Count { get; }

    internal SortedRunRows() : this([], 0) { }
    private SortedRunRows(Entry[]?[] levels, int count)
    { _levels = levels; Count = count; _ordered = new(BuildOrdered); }

    public bool TryGetValue(T key, out EngineRow<T> value)
    {
        foreach (var run in _levels)
        {
            if (run is null) continue;
            var low = 0; var high = run.Length - 1;
            while (low <= high)
            {
                var middle = low + (high - low) / 2;
                var comparison = key.CompareTo(run[middle].Key);
                if (comparison < 0) high = middle - 1;
                else if (comparison > 0) low = middle + 1;
                else
                {
                    value = run[middle].Value.GetValueOrDefault();
                    return run[middle].Value.HasValue; // A tombstone hides older values.
                }
            }
        }
        value = default; return false;
    }

    internal SortedRunRows<T> With(T key, EngineRow<T>? value)
    {
        EngineRow<T>? previous = TryGetValue(key, out var found) ? found : null;
        if (previous == value) return this;
        var count = checked(Count + ((value.HasValue ? 1 : 0) - (previous.HasValue ? 1 : 0)));
        var levels = _levels.ToList();
        var next = new[] { new Entry(key, value) };
        var tier = 0;
        while (true)
        {
            if (tier == levels.Count) levels.Add(null);
            if (levels[tier] is { } older)
            {
                levels[tier] = null; next = Merge(older, next); ++tier; continue;
            }
            // Removing tombstones is safe only if every older run participated.
            // Pinned views retain their original immutable arrays.
            if (levels.Skip(tier + 1).All(run => run is null))
                next = next.Where(entry => entry.Value.HasValue).ToArray();
            if (next.Length != 0) levels[tier] = next;
            while (levels.Count != 0 && levels[^1] is null) levels.RemoveAt(levels.Count - 1);
            return new(levels.ToArray(), count);
        }
    }

    private static Entry[] Merge(Entry[] older, Entry[] newer)
    {
        var merged = new List<Entry>(older.Length + newer.Length);
        var old = 0; var current = 0;
        while (old < older.Length && current < newer.Length)
        {
            var comparison = older[old].Key.CompareTo(newer[current].Key);
            if (comparison < 0) merged.Add(older[old++]);
            else { if (comparison == 0) ++old; merged.Add(newer[current++]); }
        }
        while (old < older.Length) merged.Add(older[old++]);
        while (current < newer.Length) merged.Add(newer[current++]);
        return merged.ToArray();
    }

    private SortedDictionary<T, EngineRow<T>> BuildOrdered()
    {
        var rows = new SortedDictionary<T, EngineRow<T>>();
        foreach (var run in _levels.Reverse())
        {
            if (run is null) continue;
            foreach (var entry in run)
            {
                if (entry.Value is { } value) rows[entry.Key] = value;
                else rows.Remove(entry.Key);
            }
        }
        return rows;
    }

    public bool ContainsKey(T key) => TryGetValue(key, out _);
    public EngineRow<T> this[T key] => TryGetValue(key, out var value) ? value : throw new KeyNotFoundException();
    public IEnumerable<T> Keys => _ordered.Value.Keys;
    public IEnumerable<EngineRow<T>> Values => _ordered.Value.Values;
    public IEnumerator<KeyValuePair<T, EngineRow<T>>> GetEnumerator() => _ordered.Value.GetEnumerator();
    IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
}
