using System.Globalization;

namespace Foundation.Data.Doublets.Cli.Graphql;

/// <summary>Which part of a triplet an id stands in: type <c>0</c> is a triplet of its own, because a triplet's type cannot be null.</summary>
public enum Position { Index, Type, Other }

public enum PlanKind { All, Empty, Match, Ids, Union, Intersect, Minus }

/// <summary>
/// How a plan gets the rows of <c>links</c> it stands for. The part of a <c>where</c> that is a conjunction of
/// equalities and relations becomes one restriction over joined variables: <c>{from: {type_id: {_eq: 5}}}</c> is
/// <c>(($v1: 5 * *) ($i: * $v1 *))</c>. <c>_in</c> and <c>_or</c> are unions of such queries, <c>_and</c> their
/// intersection and <c>_not</c> what the rest leaves. What a restriction cannot say is left to the executor.
/// </summary>
public sealed class Plan
{
    public static readonly Plan All = new(PlanKind.All);
    public static readonly Plan Empty = new(PlanKind.Empty);

    private Plan(PlanKind kind) => Kind = kind;

    public PlanKind Kind { get; }
    public string Text { get; private init; } = "";
    public List<long> IdList { get; private init; } = [];
    public List<Plan> Items { get; private init; } = [];

    public static Plan Match(string text) => new(PlanKind.Match) { Text = text };
    public static Plan Ids(List<long> ids) => new(PlanKind.Ids) { IdList = ids };
    public static Plan Union(List<Plan> items) => new(PlanKind.Union) { Items = items };
    public static Plan Intersect(List<Plan> items) => new(PlanKind.Intersect) { Items = items };
    public static Plan Minus(Plan item) => new(PlanKind.Minus) { Items = [item] };

    /// <summary>A plan as text, for tests and tracing.</summary>
    public string Render() => Kind switch
    {
        PlanKind.All => "all",
        PlanKind.Empty => "empty",
        PlanKind.Match => Text,
        PlanKind.Ids => "(ids " + string.Join(' ', IdList.Select(id => id.ToString(CultureInfo.InvariantCulture))) + ")",
        PlanKind.Union => "(union " + string.Join(' ', Items.Select(item => item.Render())) + ")",
        PlanKind.Intersect => "(intersect " + string.Join(' ', Items.Select(item => item.Render())) + ")",
        _ => "(minus " + Items[0].Render() + ")",
    };
}

/// <summary>A plan, and whether it says all the <c>where</c> does.</summary>
public readonly record struct Compiled(Plan Plan, bool Exact);

/// <summary>Plans Hasura's <c>links_bool_exp</c> as substitution queries.</summary>
public static class Planner
{
    /// <summary>Plans a <c>where</c> over <c>links</c>.</summary>
    public static Compiled Compile(Json condition, Func<Position, long, uint?> mapping)
    {
        if (condition.Kind != JsonKind.Object)
        {
            return new Compiled(Plan.All, true);
        }
        var builder = new Builder(mapping);
        builder.Nodes.Add(new Node("$i"));
        var plans = new List<Plan>();
        var exact = true;
        foreach (var (key, value) in condition.Pairs)
        {
            if (key == "id" && IdList(value) is { } ids)
            {
                plans.Add(Plan.Ids(ids));
                continue;
            }
            var mark = builder.Nodes.Count;
            var saved = (string?[])builder.Nodes[0].Parts.Clone();
            if (builder.Term(0, key, value))
            {
                continue;
            }
            builder.Truncate(mark);
            builder.Nodes[0].Parts = saved;
            var (plan, whole) = Other(key, value, mapping);
            if (plan is not null)
            {
                plans.Add(plan);
            }
            exact &= whole;
        }
        var constrained = builder.Nodes.Count > 1 || builder.Nodes[0].Parts.Any(part => part is not null);
        if (builder.Nodes.Any(node => node.Impossible))
        {
            return new Compiled(Plan.Empty, true);
        }
        if (constrained)
        {
            var root = builder.Nodes[0];
            // The most constrained first: each next pattern starts from bindings.
            var children = builder.Nodes.Skip(1).OrderByDescending(node => node.Constraints()).ToList();
            var ordered = root.Constraints() > 0
                ? new[] { root }.Concat(children)
                : children.Concat(new[] { root });
            plans.Insert(0, Plan.Match("(" + string.Join(' ', ordered.Select(node => node.Render())) + ")"));
        }
        var result = plans.Count switch
        {
            0 => Plan.All,
            1 => plans[0],
            _ => Plan.Intersect(plans),
        };
        return new Compiled(result, exact);
    }

    private static int SlotOf(string column) => column switch
    {
        "type_id" => 0,
        "from_id" => 1,
        "to_id" => 2,
        _ => -1,
    };

    /// <summary><c>from: {...}</c>: the column of this link holding the related link.</summary>
    private static int ForwardSlot(string relation) => relation switch
    {
        "type" => 0,
        "from" => 1,
        "to" => 2,
        _ => -1,
    };

    /// <summary><c>in: {...}</c>: the column of the related link holding this link.</summary>
    private static int BackwardSlot(string relation) => relation switch
    {
        "typed" => 0,
        "out" => 1,
        "in" => 2,
        _ => -1,
    };

    private static long? SingleEq(Json condition) =>
        condition.Kind == JsonKind.Object && condition.Pairs.Count == 1 && condition.Pairs[0].Key == "_eq"
            ? condition.Pairs[0].Value.AsI64()
            : null;

    private static List<long>? IdList(Json condition)
    {
        if (condition.Kind != JsonKind.Object || condition.Pairs.Count != 1)
        {
            return null;
        }
        var value = condition.Pairs[0].Value;
        switch (condition.Pairs[0].Key)
        {
            case "_eq":
                return value.AsI64() is { } id ? [id] : null;
            case "_in" when value.Kind == JsonKind.Array:
                var ids = new List<long>(value.Items.Count);
                foreach (var item in value.Items)
                {
                    if (item.AsI64() is not { } each)
                    {
                        return null;
                    }
                    ids.Add(each);
                }
                return ids;
            default:
                return null;
        }
    }

    /// <summary>A term outside the conjunction: a plan narrowing the rows, if there is one, and whether that plan says all the term does.</summary>
    private static (Plan? Plan, bool Whole) Other(string key, Json value, Func<Position, long, uint?> mapping)
    {
        switch (key)
        {
            case "_or":
                {
                    var items = value.AsList().Select(item => Compile(item, mapping)).ToList();
                    return items.Count == 0
                        ? (null, false)
                        : (Plan.Union(items.Select(item => item.Plan).ToList()), items.All(item => item.Exact));
                }
            case "_and":
                {
                    var items = value.AsList().Select(item => Compile(item, mapping)).ToList();
                    return (Plan.Intersect(items.Select(item => item.Plan).ToList()), items.All(item => item.Exact));
                }
            case "_not":
                {
                    var inner = Compile(value, mapping);
                    return inner.Exact ? (Plan.Minus(inner.Plan), true) : (null, false);
                }
            default:
                {
                    if (SlotOf(key) < 0 || IdList(value) is not { } ids)
                    {
                        return (null, false);
                    }
                    var branches = ids
                        .Select(id => Compile(Json.Object((key, Json.Object(("_eq", Json.Int(id))))), mapping).Plan)
                        .ToList();
                    return (Plan.Union(branches), true);
                }
        }
    }

    /// <summary>One triplet pattern under construction; a part is null while it is <c>*</c>.</summary>
    private sealed class Node(string index)
    {
        public string Index { get; } = index;
        public string?[] Parts { get; set; } = new string?[3];
        public bool Impossible { get; set; }

        public string Render() => $"({Index}: {Parts[0] ?? "*"} {Parts[1] ?? "*"} {Parts[2] ?? "*"})";

        public void Set(int slot, string value)
        {
            if (Parts[slot] is { } found && found != value)
            {
                Impossible = true;
            }
            else
            {
                Parts[slot] = value;
            }
        }

        public int Constraints() => Parts.Count(part => part is not null && !part.StartsWith('$'));
    }

    private sealed class Builder(Func<Position, long, uint?> mapping)
    {
        private int _nextVariable;

        public List<Node> Nodes { get; } = [];

        public void Truncate(int count)
        {
            if (Nodes.Count > count)
            {
                Nodes.RemoveRange(count, Nodes.Count - count);
            }
        }

        private string Variable() => "$v" + (++_nextVariable).ToString(CultureInfo.InvariantCulture);

        private string? Address(Position position, long id)
        {
            if (id == 0 && position == Position.Other)
            {
                return "0";
            }
            return mapping(position, id)?.ToString(CultureInfo.InvariantCulture);
        }

        /// <summary>Whether the condition is wholly a conjunction this restriction can say. When it is not, nothing is kept: the caller looks at it another way.</summary>
        private bool Pure(int node, Json condition)
        {
            var mark = Nodes.Count;
            var saved = (string?[])Nodes[node].Parts.Clone();
            if (condition.Kind != JsonKind.Object)
            {
                return false;
            }
            var pure = condition.Pairs.Count > 0 && condition.Pairs.All(pair => Term(node, pair.Key, pair.Value));
            if (!pure)
            {
                Truncate(mark);
                Nodes[node].Parts = saved;
            }
            return pure;
        }

        /// <summary>Adds the term to the node; false when the restriction cannot say it.</summary>
        public bool Term(int node, string key, Json value)
        {
            var slot = SlotOf(key);
            if (slot >= 0)
            {
                if (SingleEq(value) is not { } id)
                {
                    return false;
                }
                if (Address(slot == 0 ? Position.Type : Position.Other, id) is { } address)
                {
                    Nodes[node].Set(slot, address);
                }
                else
                {
                    Nodes[node].Impossible = true;
                }
                return true;
            }
            slot = ForwardSlot(key);
            if (slot >= 0)
            {
                var variable = Variable();
                var child = Nodes.Count;
                Nodes.Add(new Node(variable));
                if (Pure(child, value))
                {
                    Nodes[node].Set(slot, variable);
                    return true;
                }
                Truncate(child);
                return false;
            }
            slot = BackwardSlot(key);
            if (slot >= 0)
            {
                var variable = Variable();
                var child = Nodes.Count;
                var pattern = new Node(variable);
                pattern.Parts[slot] = Nodes[node].Index;
                Nodes.Add(pattern);
                if (Pure(child, value))
                {
                    return true;
                }
                Truncate(child);
                return false;
            }
            if (key == "_and")
            {
                return value.AsList().All(item =>
                    item.Kind == JsonKind.Object && item.Pairs.All(pair => Term(node, pair.Key, pair.Value)));
            }
            return false;
        }
    }
}
