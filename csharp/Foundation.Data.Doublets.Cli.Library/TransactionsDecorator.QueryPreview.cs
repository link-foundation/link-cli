using System.Numerics;

namespace Foundation.Data.Doublets.Cli;

public partial class TransactionsDecorator<TLinkAddress> where TLinkAddress : IUnsignedNumber<TLinkAddress>
{
    public virtual IReadOnlySet<TLinkAddress>? ReserveQueryAllocations(Action<INamedTypesLinks<TLinkAddress>> preview)
    {
        lock (_lock)
        {
            EnsureWriteAllowedLocked();
            if (GetType() != typeof(TransactionsDecorator<TLinkAddress>) && GetType() != typeof(TransactionsDecorator)) return null;
            return _inner.ReserveQueryAllocations(preview);
        }
    }
}
