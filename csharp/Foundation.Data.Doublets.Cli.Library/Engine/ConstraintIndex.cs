using System.Collections.Immutable;
using System.Numerics;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Private persistent roots. A rejected candidate never changes the
/// committed indexes. Only selected constraints allocate an index.</summary>
internal sealed record ConstraintIndex<T>(
    ImmutableSortedDictionary<(T Source, T Target), T>? Pairs,
    ImmutableSortedDictionary<string, T>? Names,
    ImmutableSortedDictionary<T, int>? References)
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    internal static ConstraintIndex<T>? Build(EngineOptions options, IReadOnlyDictionary<T, EngineRow<T>> rows)
    {
        if (options.ConstraintValidation != ConstraintValidation.Indexed || options.Constraints.Count == 0) return null;
        var index = new ConstraintIndex<T>(
            options.Constraints.Contains(EngineConstraint.UniquePairs) ? ImmutableSortedDictionary<(T, T), T>.Empty : null,
            options.Constraints.Contains(EngineConstraint.UniqueNames) ? ImmutableSortedDictionary.Create<string, T>(StringComparer.Ordinal) : null,
            options.Constraints.Contains(EngineConstraint.References) ? ImmutableSortedDictionary<T, int>.Empty : null);
        return index.Prepare(ImmutableSortedDictionary<T, EngineRow<T>>.Empty, rows, rows.Keys.ToArray());
    }

    internal ConstraintIndex<T> Prepare(IReadOnlyDictionary<T, EngineRow<T>> before,
        IReadOnlyDictionary<T, EngineRow<T>> after, IReadOnlyList<T> changed)
    {
        var pairs = Pairs; var names = Names; var references = References;
        // All old keys leave before any new keys enter, permitting atomic swaps.
        foreach (var id in changed)
        {
            bool present = after.TryGetValue(id, out var next);
            if (!before.TryGetValue(id, out var old)) continue;
            if (!present || Pair(old) != Pair(next))
            {
                pairs = pairs?.Remove(Pair(old));
                references = ChangeReferences(references, old, false);
            }
            if (old.Name != (present ? next.Name : null) && old.Name is { } name) names = names?.Remove(name);
        }
        foreach (var id in changed)
        {
            bool present = before.TryGetValue(id, out var old);
            if (!after.TryGetValue(id, out var next)) continue;
            if (!present || Pair(old) != Pair(next))
            {
                if (pairs is not null)
                {
                    if (pairs.TryGetValue(Pair(next), out var owner) && owner != id) throw new InvalidOperationException("Constraint: duplicate pair.");
                    pairs = pairs.SetItem(Pair(next), id);
                }
                references = ChangeReferences(references, next, true);
            }
            if (next.Name != (present ? old.Name : null) && next.Name is { } name && names is not null)
            {
                if (names.TryGetValue(name, out var owner) && owner != id) throw new InvalidOperationException("Constraint: duplicate name.");
                names = names.SetItem(name, id);
            }
        }
        if (references is not null)
            foreach (var id in changed)
            {
                if (after.TryGetValue(id, out var row))
                {
                    if ((row.Link.Source != T.Zero && !after.ContainsKey(row.Link.Source)) || (row.Link.Target != T.Zero && !after.ContainsKey(row.Link.Target)))
                        throw new InvalidOperationException("Constraint: dangling link reference.");
                }
                else if (references.ContainsKey(id)) throw new InvalidOperationException("Constraint: dangling link reference.");
            }
        return new(pairs, names, references);
    }

    private static (T, T) Pair(EngineRow<T> row) => (row.Link.Source, row.Link.Target);
    private static ImmutableSortedDictionary<T, int>? ChangeReferences(ImmutableSortedDictionary<T, int>? references, EngineRow<T> row, bool add)
    {
        if (references is null) return null;
        foreach (var id in new[] { row.Link.Source, row.Link.Target })
        {
            if (id == T.Zero) continue;
            references.TryGetValue(id, out int count);
            references = add ? references.SetItem(id, checked(count + 1)) : count > 1 ? references.SetItem(id, count - 1) : references.Remove(id);
        }
        return references;
    }
}
