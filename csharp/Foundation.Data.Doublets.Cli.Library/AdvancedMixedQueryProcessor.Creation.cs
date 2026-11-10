using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli;

public static partial class AdvancedMixedQueryProcessor<T>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    // Preserve Platform.Data.Doublets 0.18.1 CreateAndUpdate handler semantics,
    // using owned parts instead of its Link<T> list conversion (not UInt128 aware).
    private static T CreateAndUpdate(INamedTypesLinks<T> links, T source, T target, WriteHandler<T>? handler)
    {
        var constants = links.Constants;
        var state = new WriteHandlerState<T>(constants.Continue, constants.Break, handler);
        T id = T.Zero;
        state.Apply(links.Create(null, (before, after) =>
        {
            id = after![constants.IndexPart];
            return state.Handle(before!, after!);
        }));
        state.Apply(links.Update([id], new LinkParts<T>(id, source, target), handler));
        return state.Result;
    }
}
