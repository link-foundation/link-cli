using System.Diagnostics.CodeAnalysis;
using System.Numerics;
using System.Runtime.InteropServices;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Decorators;
using Platform.Data.Doublets.Memory;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Memory;

namespace Foundation.Data.Doublets.Cli;

public partial class NamedTypesDecorator<TLinkAddress>
    where TLinkAddress : struct,
        IUnsignedNumber<TLinkAddress>,
        IComparisonOperators<TLinkAddress, TLinkAddress, bool>,
        IShiftOperators<TLinkAddress, int, TLinkAddress>,
        IBitwiseOperators<TLinkAddress, TLinkAddress, TLinkAddress>,
        IMinMaxValue<TLinkAddress>
{
    /// <summary>Validate against an owned copy of both allocator images. No preview writes
    /// reach data, names or a journal. Caller-owned allocators retain the compatibility fallback.</summary>
    public virtual IReadOnlySet<TLinkAddress>? ReserveQueryAllocations(Action<INamedTypesLinks<TLinkAddress>> preview)
    {
        ArgumentNullException.ThrowIfNull(preview);
        ObjectDisposedException.ThrowIf(_disposed, this);
        // An inherited allocator or mutation override needs its own preview capability.
        if (GetType() != typeof(NamedTypesDecorator<TLinkAddress>) || _dataMemory is null || _namesMemory is null) return null;
        using var copy = CreateQueryCopy(_dataMemory, _namesMemory);
        var lease = new QueryPreviewLease<TLinkAddress>(copy);
        try
        {
            preview(lease);
            return copy.All().Select(row => new Link<TLinkAddress>(row).Index).ToHashSet();
        }
        finally { lease.Close(); }
    }

    private NamedTypesDecorator(PinnedTypesDecorator<TLinkAddress> data,
        ILinks<TLinkAddress> namesFacade, NamedLinks<TLinkAddress> names) : base(data)
    {
        PinnedTypesDecorator = data;
        _namedLinksFacade = namesFacade;
        NamedLinks = names;
        NamedLinksDatabaseFileName = string.Empty;
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope",
        Justification = "The returned copy owns both facade chains. Failure disposes the partial data chain; NamesDatabase owns names failure cleanup.")]
    private static NamedTypesDecorator<TLinkAddress> CreateQueryCopy(IResizableDirectMemory data, IResizableDirectMemory names)
    {
        var memory = CopyQueryMemory(data);
        ILinks<TLinkAddress>? facade = null;
        try
        {
            facade = new UnitedMemoryLinks<TLinkAddress>(memory, UnitedMemoryLinks<TLinkAddress>.DefaultLinksSizeStep,
                new LinksConstants<TLinkAddress>(), IndexTreeType.Default);
            facade = facade.DecorateWithAutomaticUniquenessAndUsagesRepointing();
            var pinned = new PinnedTypesDecorator<TLinkAddress>(facade);
            var (namesFacade, named) = NamesDatabase<TLinkAddress>.Open(CopyQueryMemory(names));
            return new NamedTypesDecorator<TLinkAddress>(pinned, namesFacade, named);
        }
        catch
        {
            if (facade is null) memory.Dispose();
            else LinksFacadeDisposer.Dispose(facade);
            throw;
        }
    }

    private static HeapResizableDirectMemory CopyQueryMemory(IResizableDirectMemory original)
    {
        var size = original.UsedCapacity;
        var copy = new HeapResizableDirectMemory(Math.Max(size, 8));
        try
        {
            copy.UsedCapacity = size;
            // Bound the managed staging buffer even when the mapped store is large.
            var buffer = new byte[(int)Math.Min(size, 64 * 1024)];
            for (long offset = 0; offset < size; offset += buffer.Length)
            {
                var count = (int)Math.Min(buffer.Length, size - offset);
                Marshal.Copy(original.Pointer + checked((nint)offset), buffer, 0, count);
                Marshal.Copy(buffer, 0, copy.Pointer + checked((nint)offset), count);
            }
            return copy;
        }
        catch { copy.Dispose(); throw; }
    }
}
