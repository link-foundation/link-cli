namespace Foundation.Data.Doublets.Cli;

public partial class VersionControlDecorator
{
    public virtual IReadOnlySet<uint>? ReserveQueryAllocations(Action<INamedTypesLinks<uint>> preview)
    {
        lock (_lock)
        {
            EnsureWriteAllowedLocked();
            if (GetType() != typeof(VersionControlDecorator)) return null;
            return _transactions.ReserveQueryAllocations(preview);
        }
    }
}
