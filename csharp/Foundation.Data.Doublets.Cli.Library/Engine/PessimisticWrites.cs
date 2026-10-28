using System.Numerics;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Optional engine-local point-write ownership. The engine gate protects
/// acquisition, staleness checks, publication and release; conflicts never wait.</summary>
internal sealed class PessimisticWrites<T>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    private readonly object _owner = new();
    private readonly List<T> _keys = new();

    internal void Acquire(LinksEngine<T> engine, T id, ulong baseRevision)
    {
        if (engine.WriteOwners is { } owners && owners.TryGetValue(id, out var owner)
            && !ReferenceEquals(owner, _owner))
            throw new InvalidOperationException("Busy: another transaction owns this write address.");
        if (engine.Changed is { } changed && changed.TryGetValue(id, out var revision) && revision > baseRevision)
            throw new InvalidOperationException("Conflict: a written link changed since begin.");
        if ((engine.WriteOwners ??= new()).TryAdd(id, _owner)) _keys.Add(id);
    }

    internal void Release(LinksEngine<T> engine)
    {
        foreach (var id in _keys) engine.WriteOwners!.Remove(id);
        _keys.Clear();
        if (engine.WriteOwners?.Count == 0) engine.WriteOwners = null;
    }
}
