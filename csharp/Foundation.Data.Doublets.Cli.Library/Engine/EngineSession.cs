using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>A private write set implementing the standard links and names API.
/// Raw deletion removes one row; add a usages decorator to select cascades.</summary>
public sealed class EngineSession<T> : INamedTypesLinks<T>, IDisposable
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    private readonly LinksEngine<T> _engine;
    private EngineSnapshot<T> _base;
    private readonly SortedDictionary<T, EngineRow<T>?> _writes = new();
    private bool _closed;
    private HashSet<T>? _reads;
    private bool _scanned;
    private PessimisticWrites<T>? _locks;
    // Platform.Data's default NumericType maximum predates UInt128. Supply
    // the generic-math bounds explicitly so all supported unsigned types work.
    public LinksConstants<T> Constants { get; } = new((T.One, T.MaxValue));
    internal EngineSession(LinksEngine<T> engine, EngineSnapshot<T> snapshot)
    {
        _engine = engine; _base = snapshot;
        if (engine.Options.SerializableValidation == SerializableValidation.ReadSet && engine.Options.Concurrency != ConcurrencyControl.Exclusive)
            _reads = new();
        if (engine.Options.Concurrency == ConcurrencyControl.Pessimistic) _locks = new();
    }

    private void CheckOpen()
    {
        if (_closed) throw new InvalidOperationException("Transaction is closed.");
        _engine.CheckReady();
    }
    private EngineRow<T>? Row(T id)
    {
        if (!_closed) _reads?.Add(id);
        if (_writes.TryGetValue(id, out var written)) return written;
        var snapshot = !_closed && _engine.Options.Isolation == IsolationLevel.ReadCommitted ? _engine.Current : _base;
        return snapshot.Rows.TryGetValue(id, out var row) ? row : null;
    }
    private EngineRows<T> View()
    {
        if (!_closed && _reads is not null) _scanned = true;
        var snapshot = !_closed && _engine.Options.Isolation == IsolationLevel.ReadCommitted ? _engine.Current : _base;
        var rows = new EngineRows<T>(snapshot.Rows, _engine.Options.StateStructure);
        foreach (var (id, row) in _writes)
        { if (row is { } present) rows.Set(id, present); else rows.Remove(id); }
        return rows;
    }
    public T CreateLink(T source, T target)
    {
        lock (_engine.Gate)
        {
            CheckOpen();
            T id = T.CreateChecked(_engine.Next);
            var next = _engine.NextAfter(BigInteger.CreateChecked(id));
            _locks?.Acquire(_engine, id, _base.Revision);
            _engine.Next = next;
            _writes[id] = new(new Link<T>(id, source, target), null);
            return id;
        }
    }
    public T EnsureCreated(T id)
    {
        lock (_engine.Gate)
        {
            CheckOpen();
            if (id == T.Zero) throw new ArgumentOutOfRangeException(nameof(id));
            if (Row(id) is null)
            {
                var next = _engine.NextAfter(BigInteger.CreateChecked(id));
                _locks?.Acquire(_engine, id, _base.Revision);
                _engine.Next = next;
                _writes[id] = new(new Link<T>(id, T.Zero, T.Zero), null);
            }
            return id;
        }
    }
    public Link<T>? GetLink(T id) { lock (_engine.Gate) return Row(id)?.Link; }
    public Link<T> UpdateLink(T id, T source, T target)
    {
        lock (_engine.Gate)
        {
            CheckOpen();
            var row = Row(id) ?? throw new KeyNotFoundException("Link not found.");
            _locks?.Acquire(_engine, id, _base.Revision);
            _writes[id] = row with { Link = new(id, source, target) };
            return row.Link;
        }
    }
    public Link<T> DeleteLink(T id)
    {
        lock (_engine.Gate)
        {
            CheckOpen();
            var row = Row(id) ?? throw new KeyNotFoundException("Link not found.");
            _locks?.Acquire(_engine, id, _base.Revision);
            _writes[id] = null;
            return row.Link;
        }
    }
    public string? GetName(T link) { lock (_engine.Gate) return Row(link)?.Name; }
    public T SetName(T link, string name)
    {
        ArgumentNullException.ThrowIfNull(name);
        lock (_engine.Gate)
        {
            CheckOpen();
            var row = Row(link) ?? throw new KeyNotFoundException("Link not found.");
            _locks?.Acquire(_engine, link, _base.Revision);
            _writes[link] = row with { Name = name };
            return link;
        }
    }
    public void RemoveName(T link)
    {
        lock (_engine.Gate)
        {
            CheckOpen();
            var row = Row(link) ?? throw new KeyNotFoundException("Link not found.");
            _locks?.Acquire(_engine, link, _base.Revision);
            _writes[link] = row with { Name = null };
        }
    }
    public T GetByName(string name)
    {
        lock (_engine.Gate)
            return View().Values.FirstOrDefault(row => row.Name == name).Link.Index;
    }
    internal LinksEngine<T> Owner => _engine;
    internal EngineSnapshot<T> BaseSnapshot { get => _base; set => _base = value; }
    public ulong Commit()
    {
        lock (_engine.Gate) return CommitLocked(_engine);
    }
    // Caller holds the owning engine gate; group preparation supplies private
    // tentative state with the same policies and a memory journal.
    internal ulong CommitLocked(LinksEngine<T> target)
    {
        if (_closed) throw new InvalidOperationException("Transaction is closed.");
        _closed = true;
        target.Active--;
        try
        {
            target.CheckReady();
            var writes = _writes.ToArray();
            _writes.Clear();
            if (target.Options.Isolation == IsolationLevel.Serializable
                && (target.Options.SerializableValidation == SerializableValidation.Revision || _scanned)
                && target.Current.Revision != _base.Revision)
                throw new InvalidOperationException("Conflict: database revision changed.");
            if (_reads is { } reads && target.Changed is { } generations
                && reads.Any(id => generations.TryGetValue(id, out var revision) && revision > _base.Revision))
                throw new InvalidOperationException("Conflict: a read link changed since begin.");
            if (target.Changed is { } changed && writes.Any(write => changed.TryGetValue(write.Key, out var revision) && revision > _base.Revision))
                throw new InvalidOperationException("Conflict: a written link changed since begin.");
            if (writes.Length == 0) return target.Current.Revision;
            var rows = new EngineRows<T>(target.Current.Rows, target.Options.StateStructure);
            foreach (var (id, row) in writes)
            { if (row is { } present) rows.Set(id, present); else rows.Remove(id); }
            target.Publish(rows, writes.Select(write => write.Key));
            _base = target.Current;
            return _base.Revision;
        }
        finally
        {
            _locks?.Release(target);
            _locks = null;
            _writes.Clear();
            _reads = null;
            // Validate the last committer before reclaiming generations,
            // including delete/recreate changes that restore identical rows.
            if (target.Active == 0) target.Changed = null;
        }
    }
    public void Rollback()
    {
        lock (_engine.Gate)
        {
            if (!_closed)
            {
                _closed = true; _engine.Active--; _writes.Clear(); _reads = null;
                _locks?.Release(_engine); _locks = null;
                if (_engine.Active == 0) _engine.Changed = null;
            }
        }
    }
    public void Dispose() => Rollback();

    private Link<T>[] Matching(IList<T>? restriction)
    {
        if (restriction?.Count > 3) throw new ArgumentException("A restriction has at most three parts.", nameof(restriction));
        lock (_engine.Gate)
            return View().Values.Select(row => row.Link).Where(link => restriction is null ||
                (restriction.Count == 2 ?
                (restriction[0] == Constants.Any || restriction[0] == link.Index) &&
                (restriction[1] == Constants.Any || restriction[1] == link.Source || restriction[1] == link.Target) :
                restriction.Select((part, position) => part == Constants.Any || part == (position switch
                { 0 => link.Index, 1 => link.Source, _ => link.Target })).All(match => match))).ToArray();
    }
    public T Count(IList<T>? restriction) => T.CreateChecked(Matching(restriction).Length);
    public T Each(IList<T>? restriction, ReadHandler<T>? handler)
    {
        foreach (var link in Matching(restriction))
            if (handler?.Invoke(new LinkParts<T>(link.Index, link.Source, link.Target)) == Constants.Break) return Constants.Break;
        return Constants.Continue;
    }
    public T Create(IList<T>? substitution, WriteHandler<T>? handler)
    {
        T id = CreateLink(T.Zero, T.Zero);
        return handler?.Invoke(null, new LinkParts<T>(id, T.Zero, T.Zero)) ?? Constants.Continue;
    }
    public T Update(IList<T>? restriction, IList<T>? substitution, WriteHandler<T>? handler)
    {
        if (restriction is not [var id, ..] || substitution is not [_, var source, var target])
            throw new ArgumentException("An update needs an index and a triple.");
        var before = UpdateLink(id, source, target);
        return handler?.Invoke(new LinkParts<T>(before.Index, before.Source, before.Target), new LinkParts<T>(id, source, target)) ?? Constants.Continue;
    }
    public T Delete(IList<T>? restriction, WriteHandler<T>? handler)
    {
        if (restriction is not [var id, ..]) throw new ArgumentException("A delete needs an index.");
        var before = DeleteLink(id);
        return handler?.Invoke(new LinkParts<T>(before.Index, before.Source, before.Target), null) ?? Constants.Continue;
    }
}
