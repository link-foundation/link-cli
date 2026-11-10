using System.Numerics;

namespace Foundation.Data.Doublets.Cli.Engine;

// A detached caller allocation range plus identities retained by the causal snapshot.
internal sealed class QueryAddressScope<T>(T first, T last, IEnumerable<T> retained)
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    private readonly HashSet<T> _retained = retained.ToHashSet();
    internal bool Fresh(T id) => id >= first && id <= last;
    internal void Ensure(T id)
    {
        if (id == T.Zero || (!Fresh(id) && !_retained.Contains(id)))
            throw new InvalidOperationException("Address is outside the causal query reservation.");
    }
    internal void Allocate(T id)
    {
        if (!Fresh(id)) throw new InvalidOperationException("Causal query reservation exhausted.");
    }
}
