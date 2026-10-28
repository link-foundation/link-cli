using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Engine;

// Execute the ordinary processor over owned rows; no journal, observer or
// engine handle escapes, and allocation is published only after preview success.
internal sealed class QueryPreview<T> : INamedTypesLinks<T>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    internal readonly SortedDictionary<T, EngineRow<T>> Rows;
    private readonly QueryReservations<T> _reserved = new();
    internal readonly List<T> Allocated = new();
    internal BigInteger Next;
    public LinksConstants<T> Constants { get; }

    internal QueryPreview(IEnumerable<EngineRow<T>> rows, BigInteger next, IEnumerable<T> reserved, LinksConstants<T> constants)
    {
        Rows = new(rows.ToDictionary(row => row.Link.Index));
        foreach (var id in reserved) _reserved.Add(id);
        Next = next;
        Constants = constants;
    }

    public bool TryEnsureCreated(T id)
    {
        if (!Constants.IsInternalReference(id)) throw new InvalidOperationException("Unsupported query address.");
        if (!Rows.ContainsKey(id))
        {
            Next = BigInteger.Max(Next, BigInteger.CreateChecked(id) + 1);
            Rows[id] = new(new Link<T>(id, T.Zero, T.Zero), null);
        }
        _reserved.Remove(id);
        return true;
    }

    public T Create(IList<T>? substitution, WriteHandler<T>? handler)
    {
        var fromReservation = _reserved.TryPeek(out var reserved);
        var id = fromReservation ? reserved : T.CreateChecked(Next);
        if (!Constants.IsInternalReference(id)) throw new OverflowException("Query address space exhausted.");
        if (Rows.ContainsKey(id)) throw new InvalidOperationException("Reserved query address already exists.");
        if (fromReservation) _reserved.Remove(id);
        else Next++;
        Rows[id] = new(new Link<T>(id, T.Zero, T.Zero), null);
        Allocated.Add(id);
        return handler?.Invoke(null, new LinkParts<T>(id, T.Zero, T.Zero)) ?? Constants.Continue;
    }

    public T Update(IList<T>? restriction, IList<T>? substitution, WriteHandler<T>? handler)
    {
        if (restriction is not [var id, ..] || substitution is not [_, var source, var target])
            throw new ArgumentException("An update needs an index and a triple.");
        var before = Required(id);
        Rows[id] = before with { Link = new(id, source, target) };
        return handler?.Invoke(Parts(before.Link), new LinkParts<T>(id, source, target)) ?? Constants.Continue;
    }

    public T Delete(IList<T>? restriction, WriteHandler<T>? handler)
    {
        if (restriction is not [var id, ..]) throw new ArgumentException("A delete needs an index.");
        var before = Required(id);
        Rows.Remove(id);
        return handler?.Invoke(Parts(before.Link), null) ?? Constants.Continue;
    }

    private EngineRow<T> Required(T id) => Rows.TryGetValue(id, out var row)
        ? row : throw new KeyNotFoundException("Link not found.");
    private static LinkParts<T> Parts(Link<T> row) => new(row.Index, row.Source, row.Target);

    private Link<T>[] Matching(IList<T>? restriction)
    {
        if (restriction?.Count > 3) throw new ArgumentException("A restriction has at most three parts.", nameof(restriction));
        return Rows.Values.Select(row => row.Link).Where(link => restriction is null ||
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
            if (handler?.Invoke(Parts(link)) == Constants.Break) return Constants.Break;
        return Constants.Continue;
    }

    public string? GetName(T link) => Rows.TryGetValue(link, out var row) ? row.Name : null;
    public T GetByName(string name) => Rows.Values.FirstOrDefault(row => row.Name == name).Link.Index;
    public T SetName(T link, string name)
    {
        ArgumentNullException.ThrowIfNull(name);
        Rows[link] = Required(link) with { Name = name };
        return link;
    }
    public void RemoveName(T link) => Rows[link] = Required(link) with { Name = null };
}
