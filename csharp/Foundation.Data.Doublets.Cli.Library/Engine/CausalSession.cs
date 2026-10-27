using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Engine;

public sealed record CausalWorkspaceOptions
{
    public StateStructure StateStructure { get; init; }
    public IReadOnlyList<EngineConstraint> Constraints { get; init; } = [];
    public ConstraintValidation ConstraintValidation { get; init; }
    public CausalProjectionOptions Projection { get; init; } = new();
    public Action<string>? Trace { get; init; }
}
public sealed record CausalPreparedBatch(string Token, ulong Generation, IReadOnlyList<CausalBatchEdit> Edits);

/// <summary>A frozen causal snapshot, standard private links session and one
/// explicit-token local batch. Caller-reserved addresses are not a global allocator.</summary>
public sealed class CausalSession : INamedTypesLinks<ulong>, IDisposable
{
    private readonly CausalBatch _batch;
    private readonly ulong _generation, _first, _last;
    private BigInteger _next;
    private LinksEngine<ulong>? _engine;
    private EngineSession<ulong>? _inner;
    private readonly SortedDictionary<ulong, EngineRow<ulong>> _original = new();
    private readonly Dictionary<ulong, CausalVersion[]> _retained = new();
    private EngineRow<ulong>[] _finalRows = [];
    private CausalPreparedBatch? _prepared;
    private CausalVersion[]? _result;
    private bool _disposed;
    public LinksConstants<ulong> Constants { get; } = new((1UL, ulong.MaxValue));
    private static InvalidOperationException Frozen() => new("Invalid or frozen causal session.");
    private EngineSession<ulong> Active => _inner ?? throw Frozen();
    private static CausalVersion Own(CausalVersion value) => CausalVersion.Decode(value.Encode());
    private static CausalPreparedBatch Copy(CausalPreparedBatch value) => value with
    { Edits = value.Edits.Select(edit => new CausalBatchEdit(edit.Address, edit.Observed.Select(Own).ToArray(), edit.Payload)).ToArray() };

    public CausalSession(CausalBatch batch, ulong first, ulong last, CausalConflictPolicy policy, CausalWorkspaceOptions? options = null)
    {
        options ??= new();
        if (first == 0 || first > last) throw Frozen();
        _batch = batch; var snapshot = batch.Snapshot(); _generation = snapshot.Generation;
        var projection = new CausalProjection(snapshot.Versions, policy, options.Projection);
        foreach (var group in projection.Versions().GroupBy(v => v.Address))
        {
            if (group.Key >= first && group.Key <= last) throw Frozen();
            _retained.Add(group.Key, group.ToArray());
        }
        foreach (var row in projection.Rows()) _original.Add(row.Link.Index, row);
        _first = first; _last = last; _next = first;
        _engine = new(new MemoryCommitLog(), new()
        {
            StateStructure = options.StateStructure,
            Constraints = options.Constraints,
            ConstraintValidation = options.ConstraintValidation,
            Trace = options.Trace
        });
        _engine.Validate(_original);
        _engine.Current = EngineSnapshot<ulong>.Create(0, new EngineRows<ulong>(_original, options.StateStructure));
        _engine.Next = _engine.CommittedNext = _original.Count == 0 ? BigInteger.One : BigInteger.CreateChecked(_original.Last().Key) + 1;
        _engine.ConstraintIndex = ConstraintIndex<ulong>.Build(_engine.Options, _engine.Current.Rows);
        _inner = _engine.Begin();
    }
    public EngineRow<ulong>[] Rows()
    {
        if (_inner is not { } tx) return _finalRows.ToArray();
        var rows = new List<EngineRow<ulong>>();
        tx.Each(null, parts =>
        {
            var link = new Link<ulong>(parts); rows.Add(new(link, tx.GetName(link.Index)));
            return Constants.Continue;
        });
        return rows.ToArray();
    }
    public string? GetName(ulong id) => _inner is { } tx ? tx.GetName(id) : _finalRows.FirstOrDefault(row => row.Link.Index == id).Name;
    public ulong GetByName(string name) => _inner is { } tx ? tx.GetByName(name) : _finalRows.FirstOrDefault(row => row.Name == name).Link.Index;
    public ulong SetName(ulong id, string name) => Active.SetName(id, name);
    public void RemoveName(ulong id) => Active.RemoveName(id);
    public ulong CreateLink(ulong source, ulong target)
    {
        if (_next > _last) throw Frozen(); ulong id = (ulong)_next;
        var tx = Active; tx.EnsureCreated(id); tx.UpdateLink(id, source, target); _next++; return id;
    }
    public ulong EnsureCreated(ulong id)
    {
        if (id == 0 || (!(id >= _first && id <= _last) && !_retained.ContainsKey(id))) throw Frozen();
        Active.EnsureCreated(id); if (id >= _first && id <= _last) _next = BigInteger.Max(_next, BigInteger.CreateChecked(id) + 1); return id;
    }
    public Link<ulong>? GetLink(ulong id) => _inner is { } tx ? tx.GetLink(id)
        : _finalRows.Where(row => row.Link.Index == id).Select(row => (Link<ulong>?)row.Link).FirstOrDefault();
    public Link<ulong> UpdateLink(ulong id, ulong source, ulong target) => Active.UpdateLink(id, source, target);
    public Link<ulong> DeleteLink(ulong id) => Active.DeleteLink(id);
    private CausalBatchEdit[] Finish()
    {
        var tx = Active; var engine = _engine!; _inner = null; _engine = null;
        try
        {
            tx.Commit(); _finalRows = engine.Snapshot().Rows.Values.ToArray();
            var final = _finalRows.ToDictionary(row => row.Link.Index);
            var edits = _original.Keys.Concat(final.Keys).Distinct().Order().Where(id =>
                !_original.TryGetValue(id, out var before) || !final.TryGetValue(id, out var after) || before != after)
                .Select(id => new CausalBatchEdit(id, _retained.GetValueOrDefault(id) ?? [], final.TryGetValue(id, out var row)
                    ? new(row.Link.Source, row.Link.Target, row.Name) : null)).ToArray();
            return CausalBatch.Normalize(edits).Edits;
        }
        finally { tx.Dispose(); _original.Clear(); _retained.Clear(); }
    }
    public CausalPreparedBatch? Prepared => _prepared is { } value ? Copy(value) : null;
    public CausalPreparedBatch Prepare(string token)
    {
        if (_disposed) throw Frozen();
        if (_prepared is { } request) { if (request.Token != token) throw Frozen(); return Copy(request); }
        CausalBatch.ValidateToken(token, 0);
        var edits = Finish(); CausalBatch.ValidateToken(token, edits.Length);
        _prepared = new(token, _generation, edits); return Copy(_prepared);
    }
    public CausalVersion[] Commit(string token)
    {
        var request = Prepare(token);
        _result ??= _batch.Write(request.Token, request.Generation, request.Edits);
        return _result.Select(Own).ToArray();
    }
    public void Rollback()
    {
        _inner?.Dispose(); _inner = null; _engine = null; _original.Clear(); _retained.Clear();
        _finalRows = []; _result = null; _disposed = true;
    }
    public void Dispose() => Rollback();
    private Link<ulong>[] Matching(IList<ulong>? restriction)
    {
        if (restriction?.Count > 3) throw new ArgumentException("A restriction has at most three parts.", nameof(restriction));
        return Rows().Select(row => row.Link).Where(link => restriction is null ||
            (restriction.Count == 2 ? (restriction[0] == Constants.Any || restriction[0] == link.Index) &&
                (restriction[1] == Constants.Any || restriction[1] == link.Source || restriction[1] == link.Target) :
                restriction.Select((part, position) => part == Constants.Any || part == (position switch
                { 0 => link.Index, 1 => link.Source, _ => link.Target })).All(match => match))).ToArray();
    }
    public ulong Count(IList<ulong>? restriction) => (ulong)Matching(restriction).Length;
    public ulong Each(IList<ulong>? restriction, ReadHandler<ulong>? handler)
    {
        foreach (var link in Matching(restriction))
            if (handler?.Invoke((IList<ulong>)link) == Constants.Break) return Constants.Break;
        return Constants.Continue;
    }
    public ulong Create(IList<ulong>? substitution, WriteHandler<ulong>? handler)
    {
        ulong id = CreateLink(0, 0);
        return handler?.Invoke(null, (IList<ulong>)GetLink(id)!.Value) ?? Constants.Continue;
    }
    public ulong Update(IList<ulong>? restriction, IList<ulong>? substitution, WriteHandler<ulong>? handler)
    {
        if (restriction is not [var id, ..] || substitution is not [_, var source, var target]) throw new ArgumentException("An update needs an index and a triple.");
        var before = UpdateLink(id, source, target);
        return handler?.Invoke((IList<ulong>)before, (IList<ulong>)new Link<ulong>(id, source, target)) ?? Constants.Continue;
    }
    public ulong Delete(IList<ulong>? restriction, WriteHandler<ulong>? handler)
    {
        if (restriction is not [var id, ..]) throw new ArgumentException("A delete needs an index.");
        var before = DeleteLink(id); return handler?.Invoke((IList<ulong>)before, null) ?? Constants.Continue;
    }
}
