using System.Numerics;

namespace Foundation.Data.Doublets.Cli.Engine;

public sealed partial class EngineSession<T>
    where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
{
    public bool TryEnsureCreated(T address) { EnsureCreated(address); return true; }

    public IReadOnlySet<T>? ReserveQueryAllocations(Action<INamedTypesLinks<T>> preview)
    {
        ArgumentNullException.ThrowIfNull(preview);
        lock (_engine.Gate)
        {
            CheckOpen();
            var scratch = new QueryPreview<T>(View().Values, _engine.Next, _reserved?.Addresses ?? [], Constants);
            preview(scratch);
            // Preview errors preserve the real cursor, rows and observations.
            // Successful validation may still leave reservations after a later
            // missing-reference rejection or rollback, as normal allocation does.
            _engine.Next = BigInteger.Max(_engine.Next, scratch.Next);
            foreach (var id in scratch.Allocated) (_reserved ??= new()).Add(id);
            return scratch.Rows.Keys.ToHashSet();
        }
    }
}
