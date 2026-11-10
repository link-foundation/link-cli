using System.Numerics;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Engine;

internal static class QueryMatching
{
    internal static Link<T>[] Rows<T>(IEnumerable<Link<T>> rows, IList<T>? restriction, T any, QueryLimits? limits)
        where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
    {
        if (restriction?.Count > 3) throw new ArgumentException("A restriction has at most three parts.", nameof(restriction));
        var result = new List<Link<T>>();
        foreach (var row in rows)
        {
            var matches = restriction is null || (restriction.Count == 2
                ? (restriction[0] == any || restriction[0] == row.Index) &&
                  (restriction[1] == any || restriction[1] == row.Source || restriction[1] == row.Target)
                : restriction.Select((part, position) => part == any || part == (position switch
                { 0 => row.Index, 1 => row.Source, _ => row.Target })).All(value => value));
            if (!matches) continue;
            limits?.Results(result.Count + 1);
            result.Add(row);
        }
        return result.ToArray();
    }
}
