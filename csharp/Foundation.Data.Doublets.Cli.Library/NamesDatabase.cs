using System.Numerics;
using System.Diagnostics.CodeAnalysis;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;
using Platform.Data.Doublets.Memory;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Memory;

namespace Foundation.Data.Doublets.Cli;

/// <summary>Transfers a names mapping to its facade only after initialization succeeds.</summary>
internal static class NamesDatabase<TLinkAddress>
    where TLinkAddress : struct,
        IUnsignedNumber<TLinkAddress>,
        IComparisonOperators<TLinkAddress, TLinkAddress, bool>,
        IShiftOperators<TLinkAddress, int, TLinkAddress>,
        IBitwiseOperators<TLinkAddress, TLinkAddress, TLinkAddress>,
        IMinMaxValue<TLinkAddress>
{
    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope",
        Justification = "The returned facade owns the links chain; the catch disposes the partial chain or its mapping.")]
    internal static (ILinks<TLinkAddress> Facade, NamedLinks<TLinkAddress> Names) Open(
        IResizableDirectMemory memory)
    {
        ILinks<TLinkAddress>? facade = null;
        try
        {
            facade = new UnitedMemoryLinks<TLinkAddress>(memory,
                UnitedMemoryLinks<TLinkAddress>.DefaultLinksSizeStep,
                new LinksConstants<TLinkAddress>(enableExternalReferencesSupport: true), IndexTreeType.Default);
            facade = facade.DecorateWithAutomaticUniquenessAndUsagesRepointing();
            return (facade, new UnicodeStringStorage<TLinkAddress>(facade).NamedLinks);
        }
        catch
        {
            if (facade is null) memory.Dispose();
            else LinksFacadeDisposer.Dispose(facade);
            throw;
        }
    }
}
