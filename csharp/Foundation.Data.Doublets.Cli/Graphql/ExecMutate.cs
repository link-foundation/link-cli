namespace Foundation.Data.Doublets.Cli.Graphql;

/// <summary><c>insert_T</c>, <c>update_T</c> and <c>delete_T</c> on <c>links</c> and the tables of values.</summary>
public sealed partial class Exec
{
    private static bool Writable(string table) => table == "links" || ValueKinds.OfTable(table) is not null;

    /// <summary>Rust's <c>trim_end_matches("_one")</c>: every trailing <c>_one</c> goes.</summary>
    private static string TrimOne(string table)
    {
        while (table.EndsWith("_one", StringComparison.Ordinal))
        {
            table = table[..^4];
        }
        return table;
    }

    private static string? After(string name, string prefix) =>
        name.StartsWith(prefix, StringComparison.Ordinal) ? name[prefix.Length..] : null;

    /// <summary>The mutation a field names, null when it names none.</summary>
    private Json? MutationField(Field field)
    {
        var name = field.Name;
        if (After(name, "insert_") is { } inserted && Writable(TrimOne(inserted)))
        {
            return Insert(field, inserted);
        }
        if (After(name, "update_") is { } updated)
        {
            var table = Before(updated, "_by_pk") ?? updated;
            if (Writable(table))
            {
                var byPk = table.Length != updated.Length;
                var rows = TargetRows(table, field, byPk);
                var changed = new List<Json>();
                foreach (var row in rows)
                {
                    changed.Add(Update(table, row, field));
                }
                _memo.Clear();
                return Finish(table, changed, field, byPk);
            }
        }
        if (After(name, "delete_") is { } deleted)
        {
            var table = Before(deleted, "_by_pk") ?? deleted;
            if (Writable(table))
            {
                return Delete(table, field, table.Length != deleted.Length);
            }
        }
        if (name == "reserve")
        {
            var ids = _store.Reserve(field.Argument("count", _variables)?.AsI64() ?? 0);
            var output = new List<KeyValuePair<string, Json>>();
            foreach (var item in Flatten(field.Selections, "ReserveResponse"))
            {
                output.Add(Pair(item, item.Name == "__typename"
                    ? Json.Str("ReserveResponse")
                    : Json.Array(ids.Select(Json.Int).ToList())));
            }
            return Json.Object(output);
        }
        return null;
    }

    private Json Insert(Field field, string named)
    {
        var single = named.EndsWith("_one", StringComparison.Ordinal);
        var table = TrimOne(named);
        List<Json> objects;
        if (single)
        {
            objects = [field.Argument("object", _variables) ?? Json.Null];
        }
        else
        {
            objects = field.Argument("objects", _variables) switch
            {
                { Kind: JsonKind.Array } items => items.Items,
                { } one => [one],
                null => [],
            };
        }
        var rows = new List<Json>();
        foreach (var item in objects)
        {
            rows.Add(InsertRow(table, item, null, 0));
        }
        _memo.Clear();
        if (single)
        {
            return rows.Count > 0 ? Project(table, rows[0], field.Selections) : Json.Null;
        }
        return Response(table, rows, field);
    }

    private Json Delete(string table, Field field, bool byPk)
    {
        var rows = TargetRows(table, field, byPk);
        // What is returned is read before it is gone.
        var response = Finish(table, rows, field, byPk);
        foreach (var row in rows)
        {
            var id = row.Get("id")?.AsI64() ?? 0;
            if (table == "links")
            {
                // Taken along by an earlier row of this delete, it is done.
                if (_store.TripletOf(id) is not null)
                {
                    _store.DeleteLink(_subject, id);
                }
            }
            else if (_store.Value(unchecked((uint)id)) is not null)
            {
                _store.DeleteValue(_subject, unchecked((uint)id));
            }
        }
        _memo.Clear();
        return response;
    }

    private List<Json> TargetRows(string table, Field field, bool byPk)
    {
        Json condition;
        if (byPk)
        {
            var column = PrimaryKey(table);
            var key = field.Argument("pk_columns", _variables)?.Get(column) ?? field.Argument(column, _variables) ?? Json.Null;
            condition = Json.Object((column, Json.Object(("_eq", key))));
        }
        else
        {
            condition = field.Argument("where", _variables) ?? Json.Null;
        }
        return Rows(table, condition);
    }

    private Json Finish(string table, List<Json> rows, Field field, bool byPk)
    {
        if (byPk)
        {
            return rows.Count > 0 ? Project(table, rows[0], field.Selections) : Json.Null;
        }
        return Response(table, rows, field);
    }

    /// <summary><c>{affected_rows returning {..}}</c>.</summary>
    private Json Response(string table, List<Json> rows, Field field)
    {
        var typename = $"{table}_mutation_response";
        var output = new List<KeyValuePair<string, Json>>();
        foreach (var item in Flatten(field.Selections, typename))
        {
            var value = item.Name switch
            {
                "__typename" => Json.Str(typename),
                "affected_rows" => Json.Int(rows.Count),
                "returning" => ProjectAll(table, rows, item.Selections),
                var other => throw GqlError.Validation(
                    $"field '{other}' not found in type: '{typename}'",
                    $"$.selectionSet.{field.Name}.{other}"),
            };
            output.Add(Pair(item, value));
        }
        return Json.Object(output);
    }

    /// <summary>The id a column or its nested <c>{data: ..}</c> object gives.</summary>
    private long ColumnId(Json item, string column, string relation, string? givenColumn, long given)
    {
        if (column == givenColumn)
        {
            return given;
        }
        if (item.Get(relation)?.Get("data") is { } nested)
        {
            return InsertRow("links", nested, null, 0).Get("id")?.AsI64() ?? 0;
        }
        return item.Get(column)?.AsI64() ?? 0;
    }

    /// <summary>Inserts one object and its nested objects; the given column is the one the parent of a nested array fixes.</summary>
    private Json InsertRow(string table, Json item, string? givenColumn, long given)
    {
        if (ValueKinds.OfTable(table) is { } valueKind)
        {
            var owner = givenColumn == "link_id" ? given : item.Get("link_id")?.AsI64() ?? 0;
            var row = _store.CreateValue(_subject, valueKind, owner, item.Get("value") ?? Json.Null);
            return ValueJson(row, _store.Value(row) ?? throw GqlError.Write("A value row is gone."));
        }
        var typeId = ColumnId(item, "type_id", "type", givenColumn, given);
        var fromId = ColumnId(item, "from_id", "from", givenColumn, given);
        var toId = ColumnId(item, "to_id", "to", givenColumn, given);
        var id = _store.CreateLink(_subject, item.Get("id")?.AsI64(), typeId, fromId, toId);
        foreach (var (key, kind) in new[] { ("string", ValueKind.String), ("number", ValueKind.Number), ("object", ValueKind.Object) })
        {
            if (item.Get(key)?.Get("data") is { } nested)
            {
                InsertRow(kind.Table(), nested, "link_id", id);
            }
        }
        foreach (var (key, column) in new[] { ("in", "to_id"), ("out", "from_id"), ("typed", "type_id") })
        {
            if (item.Get(key)?.Get("data") is { } nested)
            {
                foreach (var each in nested.Kind == JsonKind.Array ? nested.Items : [nested])
                {
                    InsertRow("links", each, column, id);
                }
            }
        }
        return new LinkRow(id, typeId, fromId, toId).ToJson();
    }

    private Json Update(string table, Json row, Field field)
    {
        var set = field.Argument("_set", _variables) ?? Json.Null;
        var increase = field.Argument("_inc", _variables) ?? Json.Null;
        var id = row.Get("id")?.AsI64() ?? 0;
        if (table == "links")
        {
            long Column(string name) =>
                unchecked((set.Get(name)?.AsI64() ?? row.Get(name)?.AsI64() ?? 0) + (increase.Get(name)?.AsI64() ?? 0));
            var next = new LinkRow(id, Column("type_id"), Column("from_id"), Column("to_id"));
            _store.UpdateLink(_subject, id, next.TypeId, next.FromId, next.ToId);
            return next.ToJson();
        }
        var current = row.Get("value") ?? Json.Null;
        Json value;
        if (set.Get("value") is { } given)
        {
            value = given;
        }
        else if (increase.Get("value") is { } step)
        {
            value = Json.Float((current.AsF64() ?? 0) + (step.AsF64() ?? 0));
        }
        else
        {
            value = current;
        }
        var address = unchecked((uint)id);
        _store.UpdateValue(_subject, address, value);
        return ValueJson(address, _store.Value(address) ?? throw GqlError.Write("A value row is gone."));
    }
}
