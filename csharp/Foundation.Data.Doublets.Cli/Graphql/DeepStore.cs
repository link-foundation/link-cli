using System.Globalization;
using Foundation.Data.Doublets.Cli;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Graphql;

/// <summary>The tables of values.</summary>
public enum ValueKind { String, Number, Object }

public static class ValueKinds
{
    public static readonly ValueKind[] All = [ValueKind.String, ValueKind.Number, ValueKind.Object];

    public static string Table(this ValueKind kind) => kind switch
    {
        ValueKind.String => "strings",
        ValueKind.Number => "numbers",
        _ => "objects",
    };

    public static ValueKind? OfTable(string table) => table switch
    {
        "strings" => ValueKind.String,
        "numbers" => ValueKind.Number,
        "objects" => ValueKind.Object,
        _ => null,
    };

    internal static string MarkerName(this ValueKind kind) => kind switch
    {
        ValueKind.String => "DeepString",
        ValueKind.Number => "DeepNumber",
        _ => "DeepObject",
    };

    internal static string Encode(this ValueKind kind, Json value) =>
        kind == ValueKind.String && value.Kind == JsonKind.String ? value.StringValue : value.Write();

    internal static Json Decode(this ValueKind kind, string text)
    {
        if (kind == ValueKind.String)
        {
            return Json.Str(text);
        }
        try
        {
            return Json.Parse(text);
        }
        catch (FormatException)
        {
            return Json.Null;
        }
    }
}

/// <summary>A row of <c>links</c>.</summary>
public readonly record struct LinkRow(long Id, long TypeId, long FromId, long ToId)
{
    public Json ToJson() => Json.Object(
        ("id", Json.Int(Id)),
        ("type_id", Json.Int(TypeId)),
        ("from_id", Json.Int(FromId)),
        ("to_id", Json.Int(ToId)));
}

public sealed class ValueRow(ValueKind kind, long owner, Json value)
{
    public ValueKind Kind { get; } = kind;
    public long Owner { get; } = owner;
    public Json Value { get; set; } = value;
}

/// <summary>
/// The rows of Deep over triplets. A row of <c>links</c> is a triplet with the external id <c>deep:&lt;id&gt;</c>,
/// since Deep's ids are not the addresses of the store. Deep's type <c>0</c> is a triplet of its own,
/// <c>DeepNoType</c>, because a triplet always has a type. A row of <c>strings</c>, <c>numbers</c> or <c>objects</c>
/// is a triplet <c>(row: DeepString owner 0)</c>, so deleting the owner takes it along, and its text is the name
/// <c>&lt;row&gt;:&lt;text&gt;</c>. Addresses and values are also kept in memory, rebuilt from the links when opening.
/// </summary>
public sealed class DeepStore
{
    private readonly UndoableLinksDecorator<uint> _undo;
    private readonly TripletsOverDoublets<uint> _triplets;
    private readonly PermissionPolicy<uint>? _policy;
    private readonly Dictionary<uint, TripletsPermissionsDecorator<uint>> _views = [];
    private readonly LinksExternalIds _ids;
    private readonly Dictionary<long, uint> _toTriplet = [];
    private readonly Dictionary<uint, long> _toDeep = [];
    private readonly SortedDictionary<uint, ValueRow> _values = [];
    private readonly Dictionary<(ValueKind, long), uint> _byOwner = [];
    private readonly uint[] _markers = new uint[3];
    private readonly uint _noType;
    private long _nextId = 1;

    public DeepStore(INamedTypesLinks<uint> links, PermissionPolicy<uint>? policy)
    {
        _undo = new UndoableLinksDecorator<uint>(links);
        _triplets = new TripletsOverDoublets<uint>(_undo);
        _policy = policy;
        _noType = NamedPoint("DeepNoType");
        foreach (var kind in ValueKinds.All)
        {
            _markers[(int)kind] = NamedPoint(kind.MarkerName());
        }
        _ids = new LinksExternalIds(_triplets, "deep");
        Reload();
    }

    /// <summary><c>x-hasura-admin-secret</c>: the request is not judged by the policy.</summary>
    public bool Admin { get; set; }

    /// <summary>Bumped by every committed write, for what is derived from the rows.</summary>
    public ulong Generation { get; private set; }

    /// <summary>Tables computed from the links, with the generation they were computed at.</summary>
    public Dictionary<string, (ulong At, List<Json> Rows)> Derived { get; } = [];

    /// <summary>Inside a mutation what is derived is not kept: it may yet change.</summary>
    public bool InMutation { get; private set; }

    public bool HasPolicy => _policy is not null;

    public IEnumerable<KeyValuePair<uint, ValueRow>> Values => _values;

    private uint NamedPoint(string name)
    {
        var found = _triplets.GetByName(name);
        if (found != 0)
        {
            return found;
        }
        var point = _triplets.CreateTripletPoint();
        _triplets.SetName(point, name);
        return point;
    }

    /// <summary>Rebuilds what is kept in memory from the links.</summary>
    private void Reload()
    {
        _toTriplet.Clear();
        _toDeep.Clear();
        _values.Clear();
        _byOwner.Clear();
        _nextId = 1;
        foreach (var triplet in _triplets.EachTriplet(TripletQuery<uint>.Any))
        {
            if (_ids.IdOf(triplet.Index) is { } text
                && long.TryParse(text, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var id))
            {
                _toTriplet[id] = triplet.Index;
                _toDeep[triplet.Index] = id;
                _nextId = Math.Max(_nextId, id + 1);
            }
        }
        foreach (var kind in ValueKinds.All)
        {
            foreach (var row in _triplets.EachTriplet(TripletQuery<uint>.ByType(_markers[(int)kind])))
            {
                if (!_toDeep.TryGetValue(row.From, out var owner))
                {
                    continue;
                }
                var name = _triplets.GetName(row.Index) ?? "";
                var colon = name.IndexOf(':');
                _byOwner[(kind, owner)] = row.Index;
                _values[row.Index] = new ValueRow(kind, owner, kind.Decode(colon < 0 ? "" : name[(colon + 1)..]));
            }
        }
    }

    /// <summary>The triplets as <paramref name="subject"/> sees them.</summary>
    public ITriplets<uint> View(uint subject)
    {
        if (_policy is null || Admin)
        {
            return _triplets;
        }
        if (!_views.TryGetValue(subject, out var view))
        {
            view = new TripletsPermissionsDecorator<uint>(_triplets, _policy, () => subject);
            _views[subject] = view;
        }
        return view;
    }

    public uint? TripletOf(long id) => _toTriplet.TryGetValue(id, out var found) ? found : null;

    /// <summary>The address a Deep id stands for in a position of a query.</summary>
    public uint? Address(Position position, long id) =>
        id == 0 && position == Position.Type ? _noType : TripletOf(id);

    public long? DeepOf(uint triplet) => _toDeep.TryGetValue(triplet, out var found) ? found : null;

    /// <summary>The row of a triplet, if it is a row of <c>links</c>.</summary>
    public LinkRow? RowOf(Triplet<uint> triplet)
    {
        if (DeepOf(triplet.Index) is not { } id)
        {
            return null;
        }
        long? typeId = triplet.Type == _noType ? 0 : DeepOf(triplet.Type);
        long? fromId = triplet.From == 0 ? 0 : DeepOf(triplet.From);
        long? toId = triplet.To == 0 ? 0 : DeepOf(triplet.To);
        return typeId is { } type && fromId is { } from && toId is { } to ? new LinkRow(id, type, from, to) : null;
    }

    public (uint Row, ValueRow Value)? ValueOfOwner(ValueKind kind, long owner) =>
        _byOwner.TryGetValue((kind, owner), out var row) && _values.TryGetValue(row, out var value) ? (row, value) : null;

    public ValueRow? Value(uint row) => _values.GetValueOrDefault(row);

    public List<long> Reserve(long count)
    {
        var first = _nextId;
        _nextId += Math.Max(count, 0);
        var ids = new List<long>();
        for (var id = first; id < _nextId; id++)
        {
            ids.Add(id);
        }
        return ids;
    }

    public void Begin()
    {
        InMutation = true;
        _undo.Begin();
    }

    public void Commit()
    {
        InMutation = false;
        _undo.Commit();
        Generation++;
    }

    public void Rollback()
    {
        InMutation = false;
        _undo.Rollback();
        Reload();
    }

    private uint TypeAddress(long typeId) =>
        Address(Position.Type, typeId) ?? throw new InvalidOperationException($"Foreign key violation: no link {typeId} for type_id.");

    private uint PartAddress(long id, string column)
    {
        if (id == 0)
        {
            return 0;
        }
        return TripletOf(id) ?? throw new InvalidOperationException($"Foreign key violation: no link {id} for {column}.");
    }

    /// <summary>Inserts a row of <c>links</c>; a part may name the id being inserted.</summary>
    public long CreateLink(uint subject, long? given, long typeId, long fromId, long toId)
    {
        var id = given ?? _nextId;
        if (_toTriplet.ContainsKey(id))
        {
            throw new InvalidOperationException($"Uniqueness violation: duplicate key value violates unique constraint \"links_pkey\": {id}.");
        }
        if (typeId == id || fromId == id || toId == id)
        {
            var point = View(subject).CreateTripletPoint();
            Install(id, point);
            UpdateLink(subject, id, typeId, fromId, toId);
        }
        else
        {
            var (type, from, to) = (TypeAddress(typeId), PartAddress(fromId, "from_id"), PartAddress(toId, "to_id"));
            Install(id, View(subject).CreateTriplet(type, from, to));
        }
        return id;
    }

    private void Install(long id, uint triplet)
    {
        _ids.Bind(id.ToString(CultureInfo.InvariantCulture), triplet);
        _toTriplet[id] = triplet;
        _toDeep[triplet] = id;
        _nextId = Math.Max(_nextId, id + 1);
    }

    public void UpdateLink(uint subject, long id, long typeId, long fromId, long toId)
    {
        var index = PartAddress(id, "id");
        var (type, from, to) = (TypeAddress(typeId), PartAddress(fromId, "from_id"), PartAddress(toId, "to_id"));
        View(subject).UpdateTriplet(index, type, from, to);
    }

    /// <summary>Deletes a row of <c>links</c> and all that uses it; returns the ids deleted.</summary>
    public List<long> DeleteLink(uint subject, long id)
    {
        var index = PartAddress(id, "id");
        if (_triplets.GetTriplet(index) is not { } first)
        {
            throw new InvalidOperationException($"Link {id} is not a triplet.");
        }
        var cascade = TripletsCascade.Of<uint>(_triplets, first);
        var deleted = new List<long>();
        // The external ids and value names point at the rows: they go first.
        foreach (var triplet in cascade)
        {
            if (DeepOf(triplet.Index) is { } deep)
            {
                _ids.Unbind(deep.ToString(CultureInfo.InvariantCulture));
                deleted.Add(deep);
            }
            if (_values.ContainsKey(triplet.Index))
            {
                _triplets.RemoveName(triplet.Index);
            }
        }
        View(subject).DeleteTriplet(index);
        foreach (var triplet in cascade)
        {
            if (_toDeep.Remove(triplet.Index, out var deep))
            {
                _toTriplet.Remove(deep);
            }
            if (_values.Remove(triplet.Index, out var row))
            {
                _byOwner.Remove((row.Kind, row.Owner));
            }
        }
        return deleted;
    }

    /// <summary>Whether a subject may change what hangs on the link <paramref name="owner"/>.</summary>
    private uint RequireOwnerWritable(uint subject, long owner)
    {
        var address = PartAddress(owner, "link_id");
        if (_policy is not null && !Admin)
        {
            var judge = new TripletsPermissionsDecorator<uint>(_triplets, _policy, () => subject);
            if (!_triplets.Exists(address) || !judge.CanWrite(_triplets.GetLink(address)!))
            {
                throw new InvalidOperationException($"Forbidden: {subject} may not write the value of {owner}.");
            }
        }
        return address;
    }

    private uint AddValue(ValueKind kind, long owner, uint address, Json value)
    {
        var row = _triplets.CreateTriplet(_markers[(int)kind], address, 0);
        _triplets.SetName(row, row.ToString(CultureInfo.InvariantCulture) + ":" + kind.Encode(value));
        _byOwner[(kind, owner)] = row;
        _values[row] = new ValueRow(kind, owner, value);
        return row;
    }

    public uint CreateValue(uint subject, ValueKind kind, long owner, Json value)
    {
        var address = RequireOwnerWritable(subject, owner);
        if (_byOwner.ContainsKey((kind, owner)))
        {
            throw new InvalidOperationException($"Uniqueness violation: link {owner} already has a row in {kind.Table()}.");
        }
        return AddValue(kind, owner, address, value);
    }

    public void UpdateValue(uint subject, uint row, Json value)
    {
        if (!_values.TryGetValue(row, out var found))
        {
            throw new InvalidOperationException($"No value row {row}.");
        }
        RequireOwnerWritable(subject, found.Owner);
        _triplets.SetName(row, row.ToString(CultureInfo.InvariantCulture) + ":" + found.Kind.Encode(value));
        found.Value = value;
    }

    public void DeleteValue(uint subject, uint row)
    {
        if (!_values.TryGetValue(row, out var found))
        {
            throw new InvalidOperationException($"No value row {row}.");
        }
        RequireOwnerWritable(subject, found.Owner);
        _triplets.RemoveName(row);
        _triplets.DeleteTriplet(row);
        _values.Remove(row);
        _byOwner.Remove((found.Kind, found.Owner));
    }

    /// <summary>
    /// Loads rows <c>{id, type_id, from_id, to_id, string?, number?, object?}</c>, such as a dump of
    /// <c>@deep-foundation/core</c>; rows already there are left alone, so loading twice changes nothing.
    /// Ids may refer forward, so all the points are made first.
    /// </summary>
    public int LoadPackage(Json rows)
    {
        static long Int(Json row, string key) => row.Get(key)?.AsI64() ?? 0;
        var fresh = rows.AsList().Where(row => TripletOf(Int(row, "id")) is null).ToList();
        foreach (var row in fresh)
        {
            Install(Int(row, "id"), _triplets.CreateTripletPoint());
        }
        foreach (var row in fresh)
        {
            _triplets.UpdateTriplet(
                PartAddress(Int(row, "id"), "id"),
                TypeAddress(Int(row, "type_id")),
                PartAddress(Int(row, "from_id"), "from_id"),
                PartAddress(Int(row, "to_id"), "to_id"));
        }
        foreach (var row in fresh)
        {
            foreach (var (key, kind) in new[] { ("string", ValueKind.String), ("number", ValueKind.Number), ("object", ValueKind.Object) })
            {
                if (row.Get(key) is { } value)
                {
                    AddValue(kind, Int(row, "id"), PartAddress(Int(row, "id"), "id"), value);
                }
            }
        }
        Generation++;
        return fresh.Count;
    }

    /// <summary>The grants of the policy, in Deep ids (null where the rule says <c>Anything</c>).</summary>
    public List<(bool Write, long? Subject, long? Object)> Grants()
    {
        var grants = new List<(bool, long?, long?)>();
        if (_policy is null)
        {
            return grants;
        }
        var markers = _policy.Markers;
        foreach (var (write, marker) in new[] { (false, markers.AllowRead), (true, markers.AllowWrite) })
        {
            foreach (var (subject, target) in _policy.Granted(marker))
            {
                var knownSubject = subject == markers.Anything || _toDeep.ContainsKey(subject);
                var knownObject = target == markers.Anything || _toDeep.ContainsKey(target);
                if (knownSubject && knownObject)
                {
                    grants.Add((write, subject == markers.Anything ? null : _toDeep[subject], target == markers.Anything ? null : _toDeep[target]));
                }
            }
        }
        return grants;
    }
}
