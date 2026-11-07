using System.Diagnostics;
using System.Numerics;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Optional engine-local point-write ownership. The engine gate protects
/// acquisition, staleness checks, publication and release. A refused write fails
/// fast, or with <see cref="EngineOptions.LockWait"/> waits a bounded time and
/// refuses a wait that would close a cycle.</summary>
internal sealed class PessimisticWrites<T>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    // Monitor.Wait rejects long timeouts, and the loop continues after each slice.
    private static readonly TimeSpan LongestSlice = TimeSpan.FromHours(1);
    private readonly object _owner = new();
    private readonly List<T> _keys = new();
    private bool _released;

    /// <summary>Gives up the gate until <paramref name="id"/> has no other owner, so
    /// a caller reads engine state only afterwards. Returns at once without LockWait.</summary>
    internal void WaitTurn(LinksEngine<T> engine, T id)
    {
        if (engine.Options.LockWait is not { } limit) return;
        var started = Stopwatch.GetTimestamp();
        while (engine.WriteOwners is { } owners && owners.TryGetValue(id, out var holder)
            && !ReferenceEquals(holder, _owner))
        {
            if (engine.LockEdges is { } edges && ClosesCycle(edges, holder))
                throw new InvalidOperationException("Deadlock: waiting for this write address would never end; roll back and retry.");
            var remaining = limit - Stopwatch.GetElapsedTime(started);
            if (remaining <= TimeSpan.Zero)
                throw new InvalidOperationException("Busy: timed out waiting for a write address.");
            (engine.LockEdges ??= new(ReferenceEqualityComparer.Instance))[_owner] = holder;
            try { Monitor.Wait(engine.Gate, remaining < LongestSlice ? remaining : LongestSlice); }
            finally { engine.LockEdges.Remove(_owner); }
            engine.CheckReady();
            // Another thread can only reach a waiting session by misusing it.
            if (_released) throw new InvalidOperationException("Transaction is closed.");
        }
    }

    private bool ClosesCycle(Dictionary<object, object> edges, object holder)
    {
        for (object? session = holder; session is not null; session = edges.GetValueOrDefault(session))
            if (ReferenceEquals(session, _owner)) return true;
        return false;
    }

    internal void Acquire(LinksEngine<T> engine, T id, ulong baseRevision)
    {
        WaitTurn(engine, id);
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
        _released = true;
        // Exists only while a session waits. A group commit's working engine has
        // none: it releases privately, and CommitGroup wakes everyone on its own gate.
        if (engine.LockEdges is { } edges)
        {
            foreach (var waiter in edges.Where(edge => ReferenceEquals(edge.Value, _owner)).Select(edge => edge.Key).ToArray())
                edges.Remove(waiter);
            Monitor.PulseAll(engine.Gate);
        }
    }
}
