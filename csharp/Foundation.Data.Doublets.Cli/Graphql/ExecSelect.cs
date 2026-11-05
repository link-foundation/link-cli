using Foundation.Data.Doublets.Cli;

namespace Foundation.Data.Doublets.Cli.Graphql;

/// <summary>Reading tables: <c>where</c> over plans and checks, ordering, relations, aggregates and the projection of a selection.</summary>
public sealed partial class Exec
{
    private static string? Before(string name, string suffix) =>
        name.EndsWith(suffix, StringComparison.Ordinal) ? name[..^suffix.Length] : null;

    public static string PrimaryKey(string table) => table switch
    {
        "selectors" => "item_id",
        "handlers" => "handler_id",
        "can" => "rule_id",
        _ => "id",
    };

    /// <summary>The root fields <c>T</c>, <c>T_by_pk</c> and <c>T_aggregate</c>.</summary>
    private Json? SelectField(Field field)
    {
        var name = field.Name;
        if (Tables.Names.Contains(name))
        {
            return ProjectAll(name, Select(name, field), field.Selections);
        }
        if (Before(name, "_by_pk") is { } keyed && Tables.Names.Contains(keyed))
        {
            var key = PrimaryKey(keyed);
            var id = field.Argument(key, _variables) ?? Json.Null;
            var rows = Rows(keyed, Json.Object((key, Json.Object(("_eq", id)))));
            return rows.Count > 0 ? Project(keyed, rows[0], field.Selections) : Json.Null;
        }
        if (Before(name, "_aggregate") is { } summed && Tables.Names.Contains(summed))
        {
            return Aggregate(summed, Select(summed, field), field);
        }
        return null;
    }

    /// <summary>The rows the arguments of a field select.</summary>
    private List<Json> Select(string table, Field field) =>
        Shape(table, Rows(table, field.Argument("where", _variables) ?? Json.Null), field);

    /// <summary>Applies <c>order_by</c>, <c>distinct_on</c>, <c>offset</c> and <c>limit</c>.</summary>
    private List<Json> Shape(string table, List<Json> rows, Field field)
    {
        var distinctOn = field.Argument("distinct_on", _variables) ?? Json.Null;
        var distinct = distinctOn.Kind == JsonKind.String
            ? [distinctOn.StringValue]
            : distinctOn.AsList().Select(column => column.AsStr()).OfType<string>().ToList();
        var specs = new List<(List<string> Path, string Direction)>();
        foreach (var item in OrderItems(field.Argument("order_by", _variables) ?? Json.Null))
        {
            FlattenOrder(item, [], specs);
        }
        if (specs.Count > 0)
        {
            var keyed = rows
                .Select(row => (Key: specs.Select(spec => SortValue(table, row, spec.Path, 0)).ToList(), Row: row))
                .ToList();
            var comparer = Comparer<List<Json>>.Create((left, right) =>
            {
                for (var at = 0; at < specs.Count; at++)
                {
                    var found = OrderValues(left[at], right[at], specs[at].Direction);
                    if (found != 0)
                    {
                        return found;
                    }
                }
                return 0;
            });
            rows = keyed.OrderBy(item => item.Key, comparer).Select(item => item.Row).ToList();
        }
        if (distinct.Count > 0)
        {
            var seen = new HashSet<string>();
            rows = rows
                .Where(row => seen.Add(string.Join('\0', distinct.Select(column => row.Get(column)?.Write() ?? ""))))
                .ToList();
        }
        var offset = Math.Max(field.Argument("offset", _variables)?.AsI64() ?? 0, 0);
        var limit = field.Argument("limit", _variables)?.AsI64();
        IEnumerable<Json> shaped = rows.Skip((int)Math.Min(offset, int.MaxValue));
        if (limit is { } taken)
        {
            shaped = shaped.Take((int)Math.Clamp(taken, 0, int.MaxValue));
        }
        return shaped.ToList();
    }

    private Json SortValue(string table, Json row, List<string> path, int at)
    {
        if (at >= path.Count)
        {
            return Json.Null;
        }
        if (Tables.RelationOf(table, path[at]) is { } found)
        {
            var related = Related(found, row.Get(found.Local) ?? Json.Null);
            return related.Count > 0 ? SortValue(found.Target, related[0], path, at + 1) : Json.Null;
        }
        return ColumnValue(table, row, path[at]);
    }

    /// <summary>The rows of a table passing a condition.</summary>
    public List<Json> Rows(string table, Json condition)
    {
        var kept = new List<Json>();
        if (table != "links")
        {
            foreach (var row in TableRows(table))
            {
                if (Matches(table, row, condition))
                {
                    kept.Add(row);
                }
            }
            return kept;
        }
        var compiled = Planner.Compile(condition, _store.Address);
        foreach (var found in LinkRowsOf(compiled.Plan).OrderBy(row => row.Id))
        {
            var row = found.ToJson();
            if (compiled.Exact || Matches("links", row, condition))
            {
                kept.Add(row);
            }
        }
        return kept;
    }

    private List<LinkRow> LinkRowsOf(Plan plan)
    {
        var view = _store.View(_subject);
        var rows = new List<LinkRow>();
        foreach (var address in Addresses(plan))
        {
            if (view.GetTriplet(address) is { } triplet && _store.RowOf(triplet) is { } row)
            {
                rows.Add(row);
            }
        }
        return rows;
    }

    /// <summary>The addresses of the rows a plan stands for: this is where a <c>where</c> becomes substitution queries.</summary>
    private SortedSet<uint> Addresses(Plan plan)
    {
        switch (plan.Kind)
        {
            case PlanKind.Empty:
                return [];
            case PlanKind.All:
                return new SortedSet<uint>(_store.View(_subject).EachTriplet(TripletQuery<uint>.Any)
                    .Select(triplet => triplet.Index)
                    .Where(address => _store.DeepOf(address) is not null));
            case PlanKind.Ids:
                return new SortedSet<uint>(plan.IdList.Select(_store.TripletOf).OfType<uint>());
            case PlanKind.Match:
                return new SortedSet<uint>(TripletsQueryProcessor.MatchQuery(_store.View(_subject), plan.Text, "$i")
                    .Where(address => _store.DeepOf(address) is not null));
            case PlanKind.Union:
                var union = new SortedSet<uint>();
                foreach (var item in plan.Items)
                {
                    union.UnionWith(Addresses(item));
                }
                return union;
            case PlanKind.Intersect:
                SortedSet<uint>? all = null;
                foreach (var item in plan.Items)
                {
                    var next = Addresses(item);
                    if (all is null)
                    {
                        all = next;
                    }
                    else
                    {
                        all.IntersectWith(next);
                    }
                }
                return all ?? [];
            default:
                var without = Addresses(plan.Items[0]);
                var everything = Addresses(Plan.All);
                everything.ExceptWith(without);
                return everything;
        }
    }

    /// <summary>A column, or what a computed field <c>value</c> of <c>links</c> holds.</summary>
    private Json ColumnValue(string table, Json row, string column)
    {
        if (table == "links" && column == "value")
        {
            var id = row.Get("id")?.AsI64() ?? 0;
            foreach (var kind in ValueKinds.All)
            {
                if (_store.ValueOfOwner(kind, id) is { } found)
                {
                    return Json.Object(("id", Json.Int(found.Row)), ("value", found.Value.Value));
                }
            }
            return Json.Null;
        }
        return row.Get(column) ?? Json.Null;
    }

    /// <summary>Whether a row passes a <c>bool_exp</c>.</summary>
    public bool Matches(string table, Json row, Json condition)
    {
        if (condition.Kind != JsonKind.Object)
        {
            return true;
        }
        foreach (var (key, value) in condition.Pairs)
        {
            bool passed;
            switch (key)
            {
                case "_and":
                    passed = value.AsList().All(item => Matches(table, row, item));
                    break;
                case "_or":
                    passed = value.AsList().Any(item => Matches(table, row, item));
                    break;
                case "_not":
                    passed = !Matches(table, row, value);
                    break;
                default:
                    if (Tables.RelationOf(table, key) is { } found)
                    {
                        var set = RemoteSet(found, value);
                        passed = row.Get(found.Local) is { } local && set.Contains(local.Write());
                    }
                    else if ((table == "links" && key == "value") || Tables.Columns(table).Contains(key))
                    {
                        passed = PassesAll(ColumnValue(table, row, key), key, value);
                    }
                    else
                    {
                        throw GqlError.Validation($"field '{key}' not found in type: '{table}_bool_exp'", $"$.where.{key}");
                    }
                    break;
            }
            if (!passed)
            {
                return false;
            }
        }
        return true;
    }

    private static bool PassesAll(Json column, string key, Json operators)
    {
        if (operators.Kind != JsonKind.Object)
        {
            throw GqlError.Validation("a comparison is an object", $"$.where.{key}");
        }
        try
        {
            return operators.Pairs.All(pair => Tables.Passes(column, pair.Key, pair.Value));
        }
        catch (FormatException error)
        {
            throw GqlError.Validation(error.Message, $"$.where.{key}");
        }
    }

    private HashSet<string> RemoteSet(Relation found, Json condition)
    {
        var key = $"{found.Target}|{found.Remote}|{condition.Write()}";
        if (_memo.TryGetValue(key, out var known))
        {
            return known;
        }
        var set = new HashSet<string>();
        foreach (var row in Rows(found.Target, condition))
        {
            if (row.Get(found.Remote) is { IsNull: false } value)
            {
                set.Add(value.Write());
            }
        }
        _memo[key] = set;
        return set;
    }

    /// <summary>The rows at the far end of a relation, from the value of its local column.</summary>
    private List<Json> Related(Relation found, Json local)
    {
        if (local.IsNull)
        {
            return [];
        }
        var id = local.AsI64() ?? 0;
        if (found.Target == "links")
        {
            if (_store.TripletOf(id) is not { } address)
            {
                return [];
            }
            var query = found.Remote switch
            {
                "id" => TripletQuery<uint>.ByIndex(address),
                "type_id" => TripletQuery<uint>.ByType(address),
                "from_id" => TripletQuery<uint>.ByFrom(address),
                _ => TripletQuery<uint>.ByTo(address),
            };
            return _store.View(_subject).EachTriplet(query)
                .Select(_store.RowOf)
                .OfType<LinkRow>()
                .OrderBy(row => row.Id)
                .Select(row => row.ToJson())
                .ToList();
        }
        if (ValueKinds.OfTable(found.Target) is { } kind && found.Remote == "link_id")
        {
            return _store.ValueOfOwner(kind, id) is { } value ? [ValueJson(value.Row, value.Value)] : [];
        }
        var wanted = local.Write();
        return TableRows(found.Target).Where(row => row.Get(found.Remote)?.Write() == wanted).ToList();
    }

    public Json ProjectAll(string table, List<Json> rows, List<Selection> selections) =>
        Json.Array(rows.Select(row => Project(table, row, selections)).ToList());

    private List<Json> Kept(Relation found, Json row, Field field)
    {
        var rows = Related(found, row.Get(found.Local) ?? Json.Null);
        return field.Argument("where", _variables) is { } condition
            ? rows.Where(candidate => Matches(found.Target, candidate, condition)).ToList()
            : rows;
    }

    /// <summary>A row under a selection.</summary>
    public Json Project(string table, Json row, List<Selection> selections)
    {
        var output = new List<KeyValuePair<string, Json>>();
        foreach (var field in Flatten(selections, table))
        {
            var name = field.Name;
            Json value;
            if (name == "__typename")
            {
                value = Json.Str(table);
            }
            else if (Tables.RelationOf(table, name) is { } found)
            {
                var kept = Kept(found, row, field);
                if (found.Many)
                {
                    value = ProjectAll(found.Target, Shape(found.Target, kept, field), field.Selections);
                }
                else
                {
                    value = kept.Count > 0 ? Project(found.Target, kept[0], field.Selections) : Json.Null;
                }
            }
            else if (Before(name, "_aggregate") is { } @base && Tables.RelationOf(table, @base) is { } summed)
            {
                value = Aggregate(summed.Target, Shape(summed.Target, Kept(summed, row, field), field), field);
            }
            else if (Tables.Columns(table).Contains(name) || (table == "links" && name == "value") || row.Get(name) is not null)
            {
                value = ColumnValue(table, row, name);
            }
            else
            {
                throw GqlError.Validation($"field '{name}' not found in type: '{table}'", $"$.selectionSet.{table}.selectionSet.{name}");
            }
            output.Add(Pair(field, value));
        }
        return Json.Object(output);
    }

    /// <summary><c>{aggregate {count max {..} ..} nodes {..}}</c>.</summary>
    private Json Aggregate(string table, List<Json> rows, Field field)
    {
        var output = new List<KeyValuePair<string, Json>>();
        var typename = $"{table}_aggregate";
        foreach (var item in Flatten(field.Selections, typename))
        {
            var value = item.Name switch
            {
                "__typename" => Json.Str(typename),
                "nodes" => ProjectAll(table, rows, item.Selections),
                "aggregate" => AggregateFields(table, rows, item),
                var other => throw GqlError.Validation(
                    $"field '{other}' not found in type: '{typename}'",
                    $"$.selectionSet.{field.Name}.{other}"),
            };
            output.Add(Pair(item, value));
        }
        return Json.Object(output);
    }

    private Json Count(List<Json> rows, Field item)
    {
        var selected = item.Argument("columns", _variables) ?? Json.Null;
        List<Json> chosen = selected.Kind switch
        {
            JsonKind.Null => [],
            JsonKind.Array => selected.Items,
            _ => [selected],
        };
        if (chosen.Count == 0)
        {
            return Json.Int(rows.Count);
        }
        var names = chosen.Select(column => column.AsStr()).OfType<string>().ToList();
        var present = new List<string>();
        foreach (var row in rows)
        {
            var parts = names.Select(column => row.Get(column)).Where(found => found is { IsNull: false }).Select(found => found!.Write()).ToList();
            if (parts.Count == chosen.Count)
            {
                present.Add(string.Join('|', parts));
            }
        }
        var distinct = item.Argument("distinct", _variables) is { Kind: JsonKind.Bool, BoolValue: true };
        return Json.Int(distinct ? present.Distinct().Count() : present.Count);
    }

    private Json AggregateFields(string table, List<Json> rows, Field field)
    {
        var output = new List<KeyValuePair<string, Json>>();
        var typename = $"{table}_aggregate_fields";
        foreach (var item in Flatten(field.Selections, typename))
        {
            var name = item.Name;
            Json value;
            switch (name)
            {
                case "__typename":
                    value = Json.Str(typename);
                    break;
                case "count":
                    value = Count(rows, item);
                    break;
                case "max" or "min" or "sum" or "avg":
                    var inner = $"{table}_{name}_fields";
                    var fields = new List<KeyValuePair<string, Json>>();
                    foreach (var column in Flatten(item.Selections, inner))
                    {
                        fields.Add(Pair(column, column.Name == "__typename" ? Json.Str(inner) : Summarise(name, rows, column.Name)));
                    }
                    value = Json.Object(fields);
                    break;
                default:
                    throw GqlError.Validation($"field '{name}' not found in type: '{typename}'", $"$.selectionSet.{field.Name}.{name}");
            }
            output.Add(Pair(item, value));
        }
        return Json.Object(output);
    }

    private static Json Summarise(string operation, List<Json> rows, string column)
    {
        var values = rows.Select(row => row.Get(column)).Where(found => found is { IsNull: false }).Select(found => found!).ToList();
        if (values.Count == 0)
        {
            return Json.Null;
        }
        switch (operation)
        {
            case "max":
                return values.Aggregate(values[0], (best, value) => Tables.Compare(value, best) > 0 ? value : best);
            case "min":
                return values.Aggregate(values[0], (best, value) => Tables.Compare(value, best) < 0 ? value : best);
            default:
                var sum = values.Sum(value => value.AsF64() ?? 0);
                var result = operation == "avg" ? sum / values.Count : sum;
                return result - Math.Truncate(result) == 0 && operation == "sum"
                    ? Json.Int(Json.Saturate(result))
                    : Json.Float(result);
        }
    }

    private static List<Json> OrderItems(Json order) => order.Kind switch
    {
        JsonKind.Array => order.Items,
        JsonKind.Null => [],
        _ => [order],
    };

    private static void FlattenOrder(Json item, List<string> path, List<(List<string> Path, string Direction)> specs)
    {
        if (item.Kind != JsonKind.Object)
        {
            return;
        }
        foreach (var (key, value) in item.Pairs)
        {
            path.Add(key);
            if (value.AsStr() is { } direction)
            {
                specs.Add((path.ToList(), direction));
            }
            else
            {
                FlattenOrder(value, path, specs);
            }
            path.RemoveAt(path.Count - 1);
        }
    }

    /// <summary>Postgres's defaults: ascending puts nulls last and descending first.</summary>
    private static int OrderValues(Json left, Json right, string direction)
    {
        var descending = direction.StartsWith("desc", StringComparison.Ordinal);
        var nullsFirst = direction.EndsWith("nulls_first", StringComparison.Ordinal)
            || (descending && !direction.EndsWith("nulls_last", StringComparison.Ordinal));
        if (left.IsNull || right.IsNull)
        {
            if (left.IsNull && right.IsNull)
            {
                return 0;
            }
            return left.IsNull == nullsFirst ? -1 : 1;
        }
        var found = Tables.Compare(left, right) ?? 0;
        return descending ? -found : found;
    }
}
