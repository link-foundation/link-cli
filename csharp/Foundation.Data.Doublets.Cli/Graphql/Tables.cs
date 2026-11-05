using System.Text;
using System.Text.RegularExpressions;

namespace Foundation.Data.Doublets.Cli.Graphql;

/// <summary>A relation as Hasura's metadata declares it; an array relation yields many rows, an object relation at most one.</summary>
public readonly record struct Relation(string Local, string Target, string Remote, bool Many);

/// <summary>The tables of the Deep schema: their relations and how rows are compared.</summary>
public static class Tables
{
    public static readonly string[] Names =
        ["links", "strings", "numbers", "objects", "can", "tree", "selectors", "handlers", "mp", "healthz"];

    private static Relation One(string local, string target, string remote) => new(local, target, remote, false);

    private static Relation Many(string local, string target, string remote) => new(local, target, remote, true);

    public static Relation? RelationOf(string table, string name) => (table, name) switch
    {
        ("links", "from") => One("from_id", "links", "id"),
        ("links", "to") => One("to_id", "links", "id"),
        ("links", "type") => One("type_id", "links", "id"),
        ("links", "string") => One("id", "strings", "link_id"),
        ("links", "number") => One("id", "numbers", "link_id"),
        ("links", "object") => One("id", "objects", "link_id"),
        ("links", "in") => Many("id", "links", "to_id"),
        ("links", "out") => Many("id", "links", "from_id"),
        ("links", "typed") => Many("id", "links", "type_id"),
        ("links", "can_action") => Many("id", "can", "action_id"),
        ("links", "can_object") => Many("id", "can", "object_id"),
        ("links", "can_rule") => Many("id", "can", "rule_id"),
        ("links", "can_subject") => Many("id", "can", "subject_id"),
        ("links", "down") => Many("id", "tree", "parent_id"),
        ("links", "up") => Many("id", "tree", "link_id"),
        ("links", "tree") => Many("id", "tree", "tree_id"),
        ("links", "root") => Many("id", "tree", "root_id"),
        ("links", "selected") => Many("id", "selectors", "selector_id"),
        ("links", "selectors") => Many("id", "selectors", "item_id"),
        ("links", "_by_group") => Many("id", "mp", "group_id"),
        ("links", "_by_item") => Many("id", "mp", "item_id"),
        ("links", "_by_path_item") => Many("id", "mp", "path_item_id"),
        ("links", "_by_root") => Many("id", "mp", "root_id"),
        ("strings" or "numbers" or "objects", "link") => One("link_id", "links", "id"),
        ("can", "action") => One("action_id", "links", "id"),
        ("can", "object") => One("object_id", "links", "id"),
        ("can", "rule") => One("rule_id", "links", "id"),
        ("can", "subject") => One("subject_id", "links", "id"),
        ("tree", "link") => One("link_id", "links", "id"),
        ("tree", "parent") => One("parent_id", "links", "id"),
        ("tree", "root") => One("root_id", "links", "id"),
        ("tree", "tree" or "by_tree") => One("tree_id", "links", "id"),
        ("tree", "by_link") => Many("link_id", "tree", "link_id"),
        ("tree", "by_parent") => Many("parent_id", "tree", "parent_id"),
        ("tree", "by_position") => Many("position_id", "tree", "position_id"),
        ("tree", "by_root") => Many("root_id", "tree", "root_id"),
        ("selectors", "item") => One("item_id", "links", "id"),
        ("selectors", "selector") => One("selector_id", "links", "id"),
        ("selectors", "query") => Many("query_id", "links", "id"),
        ("handlers", "dist") => One("dist_id", "links", "id"),
        ("handlers", "execution_provider") => One("execution_provider_id", "links", "id"),
        ("handlers", "handler") => One("handler_id", "links", "id"),
        ("handlers", "isolation_provider") => One("isolation_provider_id", "links", "id"),
        ("handlers", "src") => One("src_id", "links", "id"),
        _ => null,
    };

    public static string[] Columns(string table) => table switch
    {
        "links" => ["id", "type_id", "from_id", "to_id"],
        "strings" or "numbers" or "objects" => ["id", "link_id", "value"],
        "can" => ["rule_id", "action_id", "object_id", "subject_id"],
        "tree" => ["id", "link_id", "tree_id", "root_id", "parent_id", "depth", "position_id", "self"],
        "selectors" => ["item_id", "selector_id", "selector_include_id", "query_id"],
        "handlers" => ["dist_id", "src_id", "handler_id", "isolation_provider_id", "execution_provider_id"],
        "mp" => ["id", "item_id", "path_item_id", "path_item_depth", "root_id", "position_id", "group_id"],
        "healthz" => ["status"],
        _ => [],
    };

    /// <summary>Text in code-point order, the order of its UTF-8 bytes.</summary>
    public static int CompareText(string left, string right)
    {
        var shared = Math.Min(left.Length, right.Length);
        for (var at = 0; at < shared; at++)
        {
            int first = left[at], second = right[at];
            if (first == second)
            {
                continue;
            }
            if (first >= 0xD800 && second >= 0xD800)
            {
                first = first >= 0xE000 ? first - 0x800 : first + 0x2000;
                second = second >= 0xE000 ? second - 0x800 : second + 0x2000;
            }
            return first.CompareTo(second);
        }
        return left.Length.CompareTo(right.Length);
    }

    /// <summary>Numbers compare as numbers, whatever way they were written; null when the two do not compare.</summary>
    public static int? Compare(Json left, Json right)
    {
        if (left.Kind == JsonKind.String && right.Kind == JsonKind.String)
        {
            return Math.Sign(CompareText(left.StringValue, right.StringValue));
        }
        if (left.Kind == JsonKind.Bool && right.Kind == JsonKind.Bool)
        {
            return left.BoolValue.CompareTo(right.BoolValue);
        }
        if (left.AsF64() is { } first && right.AsF64() is { } second && !double.IsNaN(first) && !double.IsNaN(second))
        {
            return first.CompareTo(second);
        }
        return null;
    }

    private static int[] Fold(string value, bool insensitive) =>
        (insensitive ? value.ToLowerInvariant() : value).EnumerateRunes().Select(rune => rune.Value).ToArray();

    /// <summary>SQL's <c>LIKE</c>: <c>%</c> is any run and <c>_</c> any one character.</summary>
    public static bool Like(string text, string pattern, bool insensitive)
    {
        var characters = Fold(text, insensitive);
        var wanted = Fold(pattern, insensitive);
        int at = 0, position = 0, retryFrom = -1, star = -1;
        while (at < characters.Length)
        {
            if (position < wanted.Length && wanted[position] == '%')
            {
                retryFrom = at;
                star = position;
                position++;
            }
            else if (position < wanted.Length && (wanted[position] == '_' || wanted[position] == characters[at]))
            {
                at++;
                position++;
            }
            else if (star >= 0)
            {
                retryFrom++;
                at = retryFrom;
                position = star + 1;
            }
            else
            {
                return false;
            }
        }
        return wanted.Skip(position).All(character => character == '%');
    }

    private static bool Matches(string text, string pattern, bool insensitive)
    {
        var options = RegexOptions.NonBacktracking | RegexOptions.CultureInvariant;
        try
        {
            return Regex.IsMatch(text, pattern, insensitive ? options | RegexOptions.IgnoreCase : options);
        }
        catch (ArgumentException)
        {
            return false;
        }
        catch (NotSupportedException)
        {
            return false;
        }
    }

    private static bool Contains(Json outer, Json inner)
    {
        if (outer.Kind == JsonKind.Object && inner.Kind == JsonKind.Object)
        {
            return inner.Pairs.All(pair => outer.Get(pair.Key) is { } found && Contains(found, pair.Value));
        }
        if (outer.Kind == JsonKind.Array && inner.Kind == JsonKind.Array)
        {
            return inner.Items.All(item => outer.Items.Any(found => Contains(found, item)));
        }
        return Compare(outer, inner) == 0 || Json.Same(outer, inner);
    }

    /// <summary>Whether a column's value passes <c>{_op: argument}</c>; SQL's null compares false to everything but <c>_is_null</c>.</summary>
    /// <exception cref="FormatException">The operator is not one of Hasura's.</exception>
    public static bool Passes(Json value, string op, Json argument)
    {
        if (op == "_is_null")
        {
            return value.IsNull == (argument.Kind == JsonKind.Bool && argument.BoolValue);
        }
        if (value.IsNull)
        {
            return false;
        }
        var text = value.AsStr() ?? "";
        var pattern = argument.AsStr() ?? "";
        switch (op)
        {
            case "_eq": return Compare(value, argument) == 0 || Json.Same(value, argument);
            case "_neq": return Compare(value, argument) != 0 && !Json.Same(value, argument) && !argument.IsNull;
            case "_gt": return Compare(value, argument) > 0;
            case "_gte": return Compare(value, argument) >= 0;
            case "_lt": return Compare(value, argument) < 0;
            case "_lte": return Compare(value, argument) <= 0;
            case "_in": return argument.AsList().Any(item => Compare(value, item) == 0);
            case "_nin": return !argument.AsList().Any(item => Compare(value, item) == 0);
            case "_like": return Like(text, pattern, false);
            case "_nlike": return !Like(text, pattern, false);
            case "_ilike": return Like(text, pattern, true);
            case "_nilike": return !Like(text, pattern, true);
            case "_regex": return Matches(text, pattern, false);
            case "_nregex": return !Matches(text, pattern, false);
            case "_iregex": return Matches(text, pattern, true);
            case "_niregex": return !Matches(text, pattern, true);
            case "_contains": return Contains(value, argument);
            case "_contained_in": return Contains(argument, value);
            case "_has_key": return value.Get(pattern) is not null;
            default: throw new FormatException($"field '{op}' not found in type: 'comparison_exp'");
        }
    }
}
