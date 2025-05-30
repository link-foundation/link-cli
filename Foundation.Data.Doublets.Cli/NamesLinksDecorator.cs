using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;
using System.Numerics;

namespace Foundation.Data.Doublets.Cli
{
    public class NamesLinksDecorator<TLinkAddress> : LinksDecoratorBase<TLinkAddress>
        where TLinkAddress : IUnsignedNumber<TLinkAddress>
    {
        public NamesLinksDecorator(ILinks<TLinkAddress> links) : base(links)
        {
        }
    }
}
