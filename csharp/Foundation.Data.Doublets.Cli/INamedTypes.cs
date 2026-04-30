using System.Numerics;

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
}