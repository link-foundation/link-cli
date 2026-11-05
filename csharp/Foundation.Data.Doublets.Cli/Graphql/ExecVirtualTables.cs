using System.Globalization;

namespace Foundation.Data.Doublets.Cli.Graphql;

/// <summary>
/// Tables Deep derives from links with triggers and views. Here they are computed from the links when asked, for as
/// long as nothing is written: values and <c>can</c> always, <c>tree</c> and <c>selectors</c> from the <c>Tree*</c>
/// and <c>Selector*</c> links. <c>mp</c> is <c>tree</c> under Deep's own column names, and <c>handlers</c> is empty,
/// as Deep's handlers do not run here.
/// </summary>
public sealed partial class Exec
{
    /// <summary>
    /// Which (parent, child) edges a link of an included type makes, over its <c>from</c>, itself and its <c>to</c>.
    /// These are the groups of include types Deep's <c>isAllowSpread*</c> conditions test, in
    /// <c>1621815803572-materialized-path.ts</c>.
    /// </summary>
    private static readonly (string Spread, string[] Names)[] Spreads =
    [
        ("from-link", ["TreeIncludeDown", "TreeIncludeIn", "TreeIncludeFromCurrent", "TreeIncludeFromCurrentTo"]),
        ("to-link", ["TreeIncludeUp", "TreeIncludeIn", "TreeIncludeToCurrent", "TreeIncludeToCurrentFrom"]),
        ("link-to", ["TreeIncludeDown", "TreeIncludeOut", "TreeIncludeCurrentTo", "TreeIncludeFromCurrentTo"]),
        ("link-from", ["TreeIncludeUp", "TreeIncludeOut", "TreeIncludeCurrentFrom", "TreeIncludeToCurrentFrom"]),
    ];

    /// <summary>Every other include type: it only makes a link a member of its tree.</summary>
    private static readonly string[] MembershipOnly = ["TreeIncludeNode"];

    public static Json ValueJson(uint row, ValueRow value) => Json.Object(
        ("id", Json.Int(row)),
        ("link_id", Json.Int(value.Owner)),
        ("value", value.Value));

    /// <summary>The triplets below permissions, for the server's own lookups.</summary>
    private T Raw<T>(Func<ITriplets<uint>, T> read)
    {
        var admin = _store.Admin;
        _store.Admin = true;
        try
        {
            return read(_store.View(0));
        }
        finally
        {
            _store.Admin = admin;
        }
    }

    /// <summary>The row of a triplet below permissions.</summary>
    public LinkRow? RawRow(uint triplet) =>
        Raw(view => view.GetTriplet(triplet)) is { } found ? _store.RowOf(found) : null;

    private List<LinkRow> AllLinks() =>
        Raw(view => view.EachTriplet(TripletQuery<uint>.Any)).Select(_store.RowOf).OfType<LinkRow>().ToList();

    /// <summary>Every row of a table that is not <c>links</c>, as the subject sees them.</summary>
    public List<Json> TableRows(string table)
    {
        switch (table)
        {
            case "strings" or "numbers" or "objects":
                var kind = ValueKinds.OfTable(table);
                var output = new List<Json>();
                foreach (var (row, value) in _store.Values.ToList())
                {
                    if (value.Kind == kind && Seen(value))
                    {
                        output.Add(ValueJson(row, value));
                    }
                }
                return output;
            case "can":
                return CanRows();
            case "tree":
                return Computed("tree", TreeRows);
            case "selectors":
                return Computed("selectors", SelectorRows);
            case "mp":
                return Computed("tree", TreeRows).Select(MpRow).ToList();
            case "healthz":
                return [Json.Object(("status", Json.Str("ok")))];
            default:
                return [];
        }
    }

    /// <summary>A value is seen with its owner.</summary>
    private bool Seen(ValueRow value) =>
        !_store.HasPolicy
        || (_store.TripletOf(value.Owner) is { } address && _store.View(_subject).GetTriplet(address) is not null);

    /// <summary>A table computed from the links, kept until the next write.</summary>
    private List<Json> Computed(string table, Func<List<Json>> compute)
    {
        var generation = _store.Generation;
        if (!_store.InMutation && _store.Derived.TryGetValue(table, out var known) && known.At == generation)
        {
            return known.Rows;
        }
        var rows = compute();
        if (!_store.InMutation)
        {
            _store.Derived[table] = (generation, rows);
        }
        return rows;
    }

    /// <summary>The policy's grants as Deep's <c>can</c>: a rule on <c>Anything</c> stands as the id <c>0</c> rather than one row for each link it reaches.</summary>
    private List<Json> CanRows()
    {
        var (select, update) = (CoreId("AllowSelect"), CoreId("AllowUpdate"));
        return _store.Grants().Select(grant => Json.Object(
            ("rule_id", Json.Int(0)),
            ("action_id", Json.Int((grant.Write ? update : select) ?? 0)),
            ("object_id", Json.Int(grant.Object ?? 0)),
            ("subject_id", Json.Int(grant.Subject ?? 0)))).ToList();
    }

    /// <summary><c>tree</c>: every path from a root to a link, one row per link on it, as Deep's materialized path stores them. <c>typesTree</c> is built in: a link's parent is its type.</summary>
    private List<Json> TreeRows()
    {
        var links = AllLinks();
        var any = CoreId("Any");
        var typesTree = CoreId("typesTree");
        var spreads = new Dictionary<long, List<string>>();
        foreach (var (spread, names) in Spreads)
        {
            foreach (var name in names)
            {
                if (CoreId(name) is { } id)
                {
                    if (!spreads.TryGetValue(id, out var list))
                    {
                        spreads[id] = list = [];
                    }
                    list.Add(spread);
                }
            }
        }
        foreach (var name in MembershipOnly)
        {
            if (CoreId(name) is { } id && !spreads.ContainsKey(id))
            {
                spreads[id] = [];
            }
        }
        // tree -> included type -> the edges its links make.
        var trees = new SortedDictionary<long, Dictionary<long, HashSet<string>>>();
        foreach (var link in links)
        {
            if (link.FromId == typesTree || !spreads.TryGetValue(link.TypeId, out var made))
            {
                continue;
            }
            if (!trees.TryGetValue(link.FromId, out var included))
            {
                trees[link.FromId] = included = [];
            }
            if (!included.TryGetValue(link.ToId, out var edges))
            {
                included[link.ToId] = edges = [];
            }
            edges.UnionWith(made);
        }
        var rows = new List<Json>();
        if (typesTree is { } types)
        {
            var parents = new Dictionary<long, List<long>>();
            foreach (var link in links)
            {
                if (link.TypeId != link.Id && link.TypeId != 0)
                {
                    parents[link.Id] = [link.TypeId];
                }
            }
            PushPaths(rows, types, links.Select(link => link.Id), parents);
        }
        foreach (var (tree, includes) in trees)
        {
            var includesAny = any is { } anyId && includes.ContainsKey(anyId);
            var members = links.Where(link => includesAny || includes.ContainsKey(link.TypeId)).Select(link => link.Id).ToHashSet();
            var parents = new Dictionary<long, List<long>>();
            foreach (var link in links)
            {
                if (!members.Contains(link.Id) || !includes.TryGetValue(link.TypeId, out var made))
                {
                    continue;
                }
                foreach (var spread in made)
                {
                    var (parent, child) = spread switch
                    {
                        "from-link" => (link.FromId, link.Id),
                        "to-link" => (link.ToId, link.Id),
                        "link-to" => (link.Id, link.ToId),
                        _ => (link.Id, link.FromId),
                    };
                    if (parent != child && members.Contains(parent) && members.Contains(child))
                    {
                        if (!parents.TryGetValue(child, out var list))
                        {
                            parents[child] = list = [];
                        }
                        if (!list.Contains(parent))
                        {
                            list.Add(parent);
                        }
                    }
                }
            }
            // The rows do not depend on the order the includes were hashed in.
            foreach (var list in parents.Values)
            {
                list.Sort();
            }
            PushPaths(rows, tree, members.Order(), parents);
        }
        return rows;
    }

    /// <summary>A row of <c>tree</c> as Deep's table <c>mp</c> names its columns.</summary>
    private static Json MpRow(Json row) => Json.Object(
        ("id", row.Get("id") ?? Json.Null),
        ("item_id", row.Get("link_id") ?? Json.Null),
        ("path_item_id", row.Get("parent_id") ?? Json.Null),
        ("path_item_depth", row.Get("depth") ?? Json.Null),
        ("root_id", row.Get("root_id") ?? Json.Null),
        ("position_id", row.Get("position_id") ?? Json.Null),
        ("group_id", row.Get("tree_id") ?? Json.Null));

    /// <summary><c>selectors</c>: what a <c>SelectorInclude</c> reaches down its tree.</summary>
    private List<Json> SelectorRows()
    {
        if (CoreId("SelectorInclude") is not { } include || CoreId("SelectorTree") is not { } treeLink)
        {
            return [];
        }
        var filter = CoreId("SelectorFilter");
        var links = AllLinks();
        var reached = new Dictionary<(long, long), List<long>>();
        foreach (var row in Computed("tree", TreeRows))
        {
            var key = (row.Get("tree_id")?.AsI64() ?? 0, row.Get("parent_id")?.AsI64() ?? 0);
            if (!reached.TryGetValue(key, out var list))
            {
                reached[key] = list = [];
            }
            list.Add(row.Get("link_id")?.AsI64() ?? 0);
        }
        var treesOf = new Dictionary<long, List<long>>();
        var queryOf = new Dictionary<long, long>();
        foreach (var link in links)
        {
            if (link.TypeId == treeLink)
            {
                if (!treesOf.TryGetValue(link.FromId, out var list))
                {
                    treesOf[link.FromId] = list = [];
                }
                list.Add(link.ToId);
            }
            else if (link.TypeId == filter)
            {
                queryOf[link.FromId] = link.ToId;
            }
        }
        var rows = new List<Json>();
        foreach (var link in links.Where(link => link.TypeId == include))
        {
            foreach (var tree in treesOf.GetValueOrDefault(link.Id) ?? [])
            {
                foreach (var item in reached.GetValueOrDefault((tree, link.ToId)) ?? [])
                {
                    rows.Add(Json.Object(
                        ("item_id", Json.Int(item)),
                        ("selector_id", Json.Int(link.FromId)),
                        ("selector_include_id", Json.Int(link.Id)),
                        ("query_id", Json.Int(queryOf.GetValueOrDefault(link.FromId)))));
                }
            }
        }
        return rows;
    }

    /// <summary>The rows of every path to each node. Deep refuses cycles when they are written; here a link already on a path ends it.</summary>
    private static void PushPaths(List<Json> rows, long tree, IEnumerable<long> nodes, Dictionary<long, List<long>> parents)
    {
        foreach (var node in nodes)
        {
            var paths = new List<List<long>>();
            Walk(node, parents, [], paths);
            for (var position = 0; position < paths.Count; position++)
            {
                var path = paths[position];
                var positionId = string.Create(CultureInfo.InvariantCulture, $"{tree}-{node}-{position}");
                for (var depth = 0; depth < path.Count; depth++)
                {
                    rows.Add(Json.Object(
                        ("id", Json.Int(rows.Count + 1)),
                        ("link_id", Json.Int(node)),
                        ("tree_id", Json.Int(tree)),
                        ("root_id", Json.Int(path[0])),
                        ("parent_id", Json.Int(path[depth])),
                        ("depth", Json.Int(depth)),
                        ("position_id", Json.Str(positionId)),
                        ("self", Json.Bool(path[depth] == node))));
                }
            }
        }
    }

    private static void Walk(long node, Dictionary<long, List<long>> parents, List<long> path, List<List<long>> output)
    {
        path.Add(node);
        var above = (parents.GetValueOrDefault(node) ?? []).Where(parent => !path.Contains(parent)).ToList();
        if (above.Count == 0)
        {
            output.Add(Enumerable.Reverse(path).ToList());
        }
        foreach (var parent in above)
        {
            Walk(parent, parents, path, output);
        }
        path.RemoveAt(path.Count - 1);
    }
}
