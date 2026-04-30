using System.Numerics;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli
{
    public interface INamedTypes<TLinkAddress>
        where TLinkAddress : IUnsignedNumber<TLinkAddress>
    {
        string? GetName(TLinkAddress link);
        TLinkAddress SetName(TLinkAddress link, string name);
        TLinkAddress GetByName(string name);
        void RemoveName(TLinkAddress link);
    }

    public interface INamedTypesLinks<TLinkAddress> : ILinks<TLinkAddress>, INamedTypes<TLinkAddress>
        where TLinkAddress : IUnsignedNumber<TLinkAddress>
    {
    }
}
