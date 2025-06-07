using System.Numerics;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Data.Doublets.Decorators;
using System.Collections.Generic;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli
{
    public class SimpleLinksDecorator<TLinkAddress> : LinksDecoratorBase<TLinkAddress>
        where TLinkAddress : struct,
            IUnsignedNumber<TLinkAddress>,
            IComparisonOperators<TLinkAddress, TLinkAddress, bool>,
            IShiftOperators<TLinkAddress, int, TLinkAddress>,
            IBitwiseOperators<TLinkAddress, TLinkAddress, TLinkAddress>,
            IMinMaxValue<TLinkAddress>
    {
        public SimpleLinksDecorator(string databaseFilename)
            : base(MakeLinks(databaseFilename))
        {
        }

        public static ILinks<TLinkAddress> MakeLinks(string databaseFilename)
        {
            var links = new UnitedMemoryLinks<TLinkAddress>(databaseFilename);
            return links.DecorateWithAutomaticUniquenessAndUsagesResolution();
        }

        public override TLinkAddress Delete(IList<TLinkAddress>? restriction, WriteHandler<TLinkAddress>? handler)
        {
            var constants = _links.Constants;
            // always use a non-null handler to hit cascade uniqueness path
            WriteHandler<TLinkAddress> wrapper = (before, after) => constants.Continue;
            return _links.Delete(restriction, wrapper);
        }
    }
} 