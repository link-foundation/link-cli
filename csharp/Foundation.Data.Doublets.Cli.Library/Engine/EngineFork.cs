using System.Numerics;

namespace Foundation.Data.Doublets.Cli.Engine;

public sealed partial class LinksEngine<T>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    /// <summary>Seed an empty independent journal. Divergent allocation collisions conflict.</summary>
    public EngineFork<T> Fork(ICommitLog log, EngineOptions? options = null)
    {
        EngineSnapshot<T> snapshot;
        BigInteger next;
        lock (Gate) { CheckReady(); snapshot = Current; next = Next; }
        var engine = new LinksEngine<T>(log, options);
        lock (engine.Gate)
        {
            if (engine.Current.Revision != 0) throw new ArgumentException("Fork requires an empty branch journal.", nameof(log));
            engine.Next = next;
            engine.Publish(new(snapshot.Rows, engine.Options.StateStructure), snapshot.Rows.Keys);
        }
        return new(this, engine, engine.Current, next);
    }

    /// <summary>The caller associates the saved merge base with the same logical parent.</summary>
    public EngineFork<T> ResumeBranch(ICommitLog log, string mergeBaseRecord, EngineOptions? options = null)
    {
        lock (Gate) CheckReady();
        var (snapshot, next) = RevisionJournal<T>.Decode(mergeBaseRecord);
        if (!log.Read().Select(RevisionJournal<T>.Decode).Any(record => record.Item1.Revision == snapshot.Revision
            && record.Item2 == next && record.Item1.Rows.SequenceEqual(snapshot.Rows)))
            throw new InvalidDataException("Merge base is not a committed branch revision.");
        var engine = new LinksEngine<T>(log, options);
        return new(this, engine, snapshot.Adapt(engine.Options.StateStructure), next);
    }
}

/// <summary>Independent committed history and atomic row-level three-way merges.</summary>
public sealed class EngineFork<T>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    private readonly object _mergeGate = new();
    private readonly LinksEngine<T> _parent;
    private EngineSnapshot<T> _base;
    private BigInteger _baseNext;
    public LinksEngine<T> Engine { get; }
    internal EngineFork(LinksEngine<T> parent, LinksEngine<T> engine, EngineSnapshot<T> snapshot, BigInteger next)
    { _parent = parent; Engine = engine; _base = snapshot; _baseNext = next; }

    /// <summary>Persist with application metadata after each successful merge.</summary>
    public string MergeBaseRecord
    { get { lock (_mergeGate) return RevisionJournal<T>.Encode(_base, _baseNext); } }

    public ulong Merge()
    {
        lock (_mergeGate)
        {
            EngineSnapshot<T> incoming;
            BigInteger next;
            lock (Engine.Gate) { Engine.CheckReady(); incoming = Engine.Current; next = Engine.CommittedNext; }
            lock (_parent.Gate)
            {
                _parent.CheckReady();
                if (_parent.Options.Concurrency == ConcurrencyControl.Exclusive && _parent.Active > 0)
                    throw new InvalidOperationException("Busy: an exclusive transaction is active.");
                var rows = new EngineRows<T>(_parent.Current.Rows, _parent.Options.StateStructure);
                var changes = new List<T>();
                foreach (var id in _base.Rows.Keys.Concat(incoming.Rows.Keys).Distinct().Order())
                {
                    var before = Row(_base.Rows, id);
                    var after = Row(incoming.Rows, id);
                    if (before == after) continue;
                    var current = Row(rows, id);
                    if (current != before && current != after)
                        throw new InvalidOperationException("Conflict: branch and parent changed the same row.");
                    if (current == after) continue;
                    if (_parent.WriteOwners?.ContainsKey(id) == true)
                        throw new InvalidOperationException("Busy: another transaction owns this write address.");
                    if (after is { } value) rows.Set(id, value); else rows.Remove(id);
                    changes.Add(id);
                }
                _parent.Next = BigInteger.Max(_parent.Next, next);
                if (changes.Count > 0 || _parent.Next != _parent.CommittedNext) _parent.Publish(rows, changes);
                _base = incoming;
                _baseNext = next;
                return _parent.Current.Revision;
            }
        }
    }

    private static EngineRow<T>? Row(IReadOnlyDictionary<T, EngineRow<T>> rows, T id)
        => rows.TryGetValue(id, out var row) ? row : null;
}
