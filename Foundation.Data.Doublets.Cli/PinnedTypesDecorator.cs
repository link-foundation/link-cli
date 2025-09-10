using System;
using System.Collections;
using System.Collections.Generic;
using System.Numerics;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli
{
    public class PinnedTypesDecorator<TLinkAddress> : LinksDecoratorBase<TLinkAddress>, IPinnedTypes<TLinkAddress>
        where TLinkAddress : struct, 
            IUnsignedNumber<TLinkAddress>,
            IComparisonOperators<TLinkAddress, TLinkAddress, bool>,
            IShiftOperators<TLinkAddress, int, TLinkAddress>,
            IBitwiseOperators<TLinkAddress, TLinkAddress, TLinkAddress>,
            IMinMaxValue<TLinkAddress>
    {
        private readonly IPinnedTypes<TLinkAddress> _pinnedTypes;

        public PinnedTypesDecorator(ILinks<TLinkAddress> links) : base(links)
        {
            _pinnedTypes = new PinnedTypes<TLinkAddress>(links);
        }

        // Implement IPinnedTypes interface
        public IEnumerator<TLinkAddress> GetEnumerator()
        {
            return _pinnedTypes.GetEnumerator();
        }

        IEnumerator IEnumerable.GetEnumerator()
        {
            return GetEnumerator();
        }

        public void Deconstruct(out TLinkAddress type1, out TLinkAddress type2, out TLinkAddress type3)
        {
            _pinnedTypes.Deconstruct(out type1, out type2, out type3);
        }
    }
}