using System.Collections;
using System.Numerics;

namespace Foundation.Data.Doublets.Cli;

// An owned standard three-part row. The upstream Link<T> list helpers initialize
// default constants through reflection that predates UInt128. Use explicit parts
// at the generic interface boundary; store-selected constants remain authoritative.
internal readonly struct LinkParts<T> : IList<T>, IEquatable<LinkParts<T>>
    where T : IUnsignedNumber<T>
{
    public readonly T Index;
    public readonly T Source;
    public readonly T Target;

    public LinkParts(T index, T source, T target) => (Index, Source, Target) = (index, source, target);
    public LinkParts(IList<T>? row)
    {
        Index = row is { Count: >= 1 and <= 3 } ? row[0] : T.Zero;
        Source = row is { Count: >= 2 and <= 3 } ? row[1] : T.Zero;
        Target = row is { Count: 3 } ? row[2] : T.Zero;
    }

    public static implicit operator T[](LinkParts<T> row) => [row.Index, row.Source, row.Target];

    public int Count => 3;
    public bool IsReadOnly => true;
    public T this[int index]
    {
        get => index switch { 0 => Index, 1 => Source, 2 => Target, _ => throw new ArgumentOutOfRangeException(nameof(index)) };
        set => throw new NotSupportedException();
    }
    public int IndexOf(T item) => Index == item ? 0 : Source == item ? 1 : Target == item ? 2 : -1;
    public bool Contains(T item) => IndexOf(item) >= 0;
    public void CopyTo(T[] array, int arrayIndex) => new[] { Index, Source, Target }.CopyTo(array, arrayIndex);
    public IEnumerator<T> GetEnumerator() { yield return Index; yield return Source; yield return Target; }
    IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
    public bool Equals(LinkParts<T> other) => Index == other.Index && Source == other.Source && Target == other.Target;
    public override bool Equals(object? other) => other is LinkParts<T> row && Equals(row);
    public override int GetHashCode() => HashCode.Combine(Index, Source, Target);
    public override string ToString() => Index == T.Zero ? $"({Source}->{Target})" : $"({Index}: {Source}->{Target})";
    public void Add(T item) => throw new NotSupportedException();
    public void Clear() => throw new NotSupportedException();
    public void Insert(int index, T item) => throw new NotSupportedException();
    public bool Remove(T item) => throw new NotSupportedException();
    public void RemoveAt(int index) => throw new NotSupportedException();
}
