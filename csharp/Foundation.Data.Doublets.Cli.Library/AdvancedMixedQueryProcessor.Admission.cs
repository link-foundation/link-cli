using System.Numerics;

namespace Foundation.Data.Doublets.Cli;

public static partial class AdvancedMixedQueryProcessor<T>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    private static void PreviewQuery(INamedTypesLinks<T> links, string? query)
    {
        var changes = new Dictionary<T, (LinkParts<T> Before, LinkParts<T> After)>();
        var limits = links.QueryLimits;
        ProcessQuery(links, new Options
        {
            Query = query,
            SkipReferenceValidation = true,
            ChangesHandler = limits is null ? null : (before, after) =>
            {
                var first = new LinkParts<T>(before); var last = new LinkParts<T>(after);
                var id = first.Index == T.Zero ? last.Index : first.Index;
                if (changes.TryGetValue(id, out var previous)) first = previous.Before;
                else limits.Results(changes.Count + 1);
                changes[id] = (first, last);
                return links.Constants.Continue;
            }
        });
        limits?.Reply(links, changes.Values);
    }
}
