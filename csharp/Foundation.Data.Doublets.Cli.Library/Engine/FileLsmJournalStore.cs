namespace Foundation.Data.Doublets.Cli.Engine;

public sealed record FileLsmOptions(LsmStoreOptions? Store = null, ulong MaxPages = 16384,
    bool MirroredRoot = false, bool MirroredBanks = false, bool MirroredTables = false)
{
    internal LsmStoreOptions StoreOptions => Store ?? new();
}
/// <summary>Selected exclusive native directory facade. Validates all selected
/// physical and semantic state read-only before constructor recovery effects.
/// Close attempts all children before releasing the outer fence; uncertain live
/// handles retain it without raw-close retries. Never steals crashed ownership.
/// Windows does not advertise durable namespace flushing. Modes stay fixed.</summary>
public sealed class FileLsmJournalStore : IJournalMaintenanceStore, IDisposable
{
    private readonly object _gate = new();
    private readonly FileLsmFiles _files;
    private PhysicalLsmJournalStore? _store;
    private bool _closed;
    private FileLsmJournalStore(string directory, FileLsmOptions options, FileLsmIo io)
    {
        (options.StoreOptions.Limits ?? new()).Validate();
        if (options.MaxPages is < 1 or > 65536 || options.StoreOptions.MaxNamespaceEntries is < 2 or > 65536) throw new ArgumentOutOfRangeException(nameof(options));
        _files = new(directory, options, io);
        try
        {
            var names = _files.List(options.StoreOptions.MaxNamespaceEntries); var devices = _files.Wal(readOnly: true);
            var records = CheckpointPagedCommitLog.Inspect(devices.Root, devices.First, devices.Second, options.MaxPages);
            if (records.Count == 0)
            { if (names.Count != 0) throw new InvalidDataException("Uninitialized native LSM namespace contains tables."); }
            else
            { using var validation = PhysicalLsmJournalStore.Open(new NativeLsmReadOnlyWal(records), _files.Provider(readOnly: true), options.StoreOptions); }
            _files.CloseReaders(); devices = _files.Wal(readOnly: false);
            var wal = new CheckpointPagedCommitLog(devices.Root, devices.First, devices.Second, options.MaxPages);
            _store = PhysicalLsmJournalStore.Open(wal, _files.Provider(readOnly: false), options.StoreOptions);
        }
        catch (Exception cause)
        { try { Dispose(); } catch (Exception cleanup) { throw new AggregateException("Native LSM open and cleanup failed.", cause, cleanup); } throw; }
    }
    public static FileLsmJournalStore Open(string directory, FileLsmOptions? options = null) => new(directory, options ?? new(), new());
    internal static FileLsmJournalStore Open(string directory, FileLsmOptions options, FileLsmIo io) => new(directory, options, io);
    private T Run<T>(Func<PhysicalLsmJournalStore, T> action)
    { lock (_gate) { ObjectDisposedException.ThrowIf(_closed, this); return action(_store!); } }
    public bool SupportsDurableFlush => Run(store => store.SupportsDurableFlush);
    public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys) => Run(store => store.Read(keys));
    public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates) => Run(store => store.CompareExchange(keys, expected, updates));
    public bool CompareDelete(string key, ulong expectedGeneration) => Run(store => store.CompareDelete(key, expectedGeneration));
    public IReadOnlyList<JournalNamespaceEntry> ScanNamespace(JournalScanOptions? options = null) => Run(store => store.ScanNamespace(options));
    public void Flush() => Run(store => { store.Flush(); return true; });
    public void Compact() => Run(store => { store.Compact(); return true; });
    public void Dispose()
    {
        lock (_gate)
        {
            if (_closed) { _files.Close(); return; }
            _closed = true; var errors = new List<Exception>();
            try { _store?.Dispose(); } catch (Exception error) { errors.Add(error); }
            _store = null; _files.Close(errors);
        }
    }
}
