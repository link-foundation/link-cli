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
    // Platform.Data's default NumericType maximum predates UInt128. Supply
    // the generic-math bounds explicitly so all supported unsigned types work.
    public LinksConstants<T> Constants { get; } = new((T.One, T.MaxValue));
    internal EngineSession(LinksEngine<T> engine, EngineSnapshot<T> snapshot) { _engine = engine; _base = snapshot; }

    private void CheckOpen()
    {
        if (_closed) throw new InvalidOperationException("Transaction is closed.");
        _engine.CheckReady();
    }
    private EngineRow<T>? Row(T id)
    {
        if (_writes.TryGetValue(id, out var written)) return written;
        var snapshot = !_closed && _engine.Options.Isolation == IsolationLevel.ReadCommitted ? _engine.Current : _base;
        return snapshot.Rows.TryGetValue(id, out var row) ? row : null;
    }
    private SortedDictionary<T, EngineRow<T>> View()
    {
        var snapshot = !_closed && _engine.Options.Isolation == IsolationLevel.ReadCommitted ? _engine.Current : _base;
        var rows = new SortedDictionary<T, EngineRow<T>>(snapshot.Rows.ToDictionary());
        foreach (var (id, row) in _writes)
        { if (row is { } present) rows[id] = present; else rows.Remove(id); }
        return rows;
    }
    public T CreateLink(T source, T target)
    {
        lock (_engine.Gate)
        {
            CheckOpen();
            T id = _engine.Allocate();
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
                _engine.Advance(BigInteger.CreateChecked(id));
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
            _writes[link] = row with { Name = null };
        }
    }
    public T GetByName(string name)
    {
        lock (_engine.Gate)
            return View().Values.FirstOrDefault(row => row.Name == name).Link.Index;
    }
    public ulong Commit()
    {
        lock (_engine.Gate)
        {
            CheckOpen();
            _closed = true;
            _engine.Active--;
            var writes = _writes.ToArray();
            _writes.Clear();
            if (_engine.Options.Isolation == IsolationLevel.Serializable && _engine.Current.Revision != _base.Revision)
                throw new InvalidOperationException("Conflict: database revision changed.");
            if (writes.Any(write => _engine.Changed.TryGetValue(write.Key, out var revision) && revision > _base.Revision))
                throw new InvalidOperationException("Conflict: a written link changed since begin.");
            if (writes.Length == 0) return _engine.Current.Revision;
            var rows = new SortedDictionary<T, EngineRow<T>>(_engine.Current.Rows.ToDictionary());
            foreach (var (id, row) in writes)
            { if (row is { } present) rows[id] = present; else rows.Remove(id); }
            _engine.Publish(rows, writes.Select(write => write.Key));
            _base = _engine.Current;
            return _base.Revision;
        }
    }
    public void Rollback()
    {
        lock (_engine.Gate)
        {
            if (!_closed) { _closed = true; _engine.Active--; _writes.Clear(); }
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
            if (handler?.Invoke((IList<T>)link) == Constants.Break) return Constants.Break;
        return Constants.Continue;
    }
    public T Create(IList<T>? substitution, WriteHandler<T>? handler)
    {
        T id = CreateLink(T.Zero, T.Zero);
        return handler?.Invoke(null, (IList<T>)GetLink(id)!.Value) ?? Constants.Continue;
    }
    public T Update(IList<T>? restriction, IList<T>? substitution, WriteHandler<T>? handler)
    {
        if (restriction is not [var id, ..] || substitution is not [_, var source, var target])
            throw new ArgumentException("An update needs an index and a triple.");
        var before = UpdateLink(id, source, target);
        return handler?.Invoke((IList<T>)before, (IList<T>)new Link<T>(id, source, target)) ?? Constants.Continue;
    }
    public T Delete(IList<T>? restriction, WriteHandler<T>? handler)
    {
        if (restriction is not [var id, ..]) throw new ArgumentException("A delete needs an index.");
        var before = DeleteLink(id);
        return handler?.Invoke((IList<T>)before, null) ?? Constants.Continue;
    }
}
