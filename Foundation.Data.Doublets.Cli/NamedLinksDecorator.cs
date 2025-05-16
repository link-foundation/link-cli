using System.Numerics;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;
using Platform.Data.Doublets.Memory.United.Generic;

namespace Foundation.Data.Doublets.Cli
{
  public class NamedLinksDecorator<TLinkAddress> : LinksDecoratorBase<TLinkAddress>
    where TLinkAddress : IUnsignedNumber<TLinkAddress>, IShiftOperators<TLinkAddress,int,TLinkAddress>, IBitwiseOperators<TLinkAddress,TLinkAddress,TLinkAddress>, IMinMaxValue<TLinkAddress>, IComparisonOperators<TLinkAddress, TLinkAddress, bool>
  {
    private static ILinks<TLinkAddress> Create(string db)
    {
      var links = new UnitedMemoryLinks<TLinkAddress>(db);
      var decoratedLinks = links.DecorateWithAutomaticUniquenessAndUsagesResolution();
      return decoratedLinks;
    }

    public NamedLinksDecorator(string db) : base(Create(db))
    {
    }
  }
}