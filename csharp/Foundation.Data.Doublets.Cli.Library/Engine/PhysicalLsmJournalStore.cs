using System.Globalization;
using System.Text;
using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Exclusive synchronous table namespace. List returns every canonical
/// numeric identity in order or fails within its bound. Create never reuses a
/// name; Open never creates missing files. Successful create/delete persist the
/// directory entry. Close releases a lease without committing staged repairs and
/// must permit retry after an uncertain result. The caller owns this provider.</summary>
public interface ILsmTableNamespace
{
    bool SupportsDurableFlush { get; }
    IReadOnlyList<ulong> List(int maxEntries);
    IPageDevice Create(ulong id);
    IPageDevice Open(ulong id);
    void Close(IPageDevice device);
    void Delete(ulong id);
}
public sealed record LsmStoreOptions(LsmOptions? Limits = null, int MaxNamespaceEntries = 256);

/// <summary>Selected bounded physical LSM with foreground full compaction.
/// Retains only metadata for disk values. Owns table leases, but never disposes
/// the caller's WAL or namespace. An uncertain mutation fences all operations
/// until actual reopening. Dispose retries failed lease cleanup.</summary>
public sealed partial class PhysicalLsmJournalStore : IJournalMaintenanceStore, IDisposable
{
    private static readonly UTF8Encoding Utf8 = new(false, true);
    private sealed record Index(ulong Generation, bool Tombstone, long Bytes, ulong Owner);
    private readonly object _gate = new();
    private readonly IReplaceableCommitLog _wal;
    private readonly ILsmTableNamespace _provider;
    private readonly LsmOptions _limits;
    private readonly int _maxNamespace;
    private readonly bool _tableDurable;
    private readonly SortedDictionary<string, Index> _index = new(StringComparer.Ordinal);
    private readonly Dictionary<ulong, SortedJournalTable> _tables = new();
    private readonly Dictionary<ulong, IPageDevice> _leases = new();
    private LsmWalState _state = new(0, 0, 0, Array.Empty<LsmTableDescriptor>(), Array.Empty<SortedJournalEntry>());
    private int _recordCount;
    private long _walBytes;
    private bool _busy, _closed, _fenced;
    public bool SupportsDurableFlush { get; }
    private PhysicalLsmJournalStore(IReplaceableCommitLog wal, ILsmTableNamespace provider, LsmStoreOptions options)
    {
        _wal = wal; _provider = provider; _limits = options.Limits ?? new(); _maxNamespace = options.MaxNamespaceEntries;
        _tableDurable = provider.SupportsDurableFlush; SupportsDurableFlush = wal.SupportsDurableFlush && _tableDurable;
    }
    public static PhysicalLsmJournalStore Open(IReplaceableCommitLog wal, ILsmTableNamespace provider, LsmStoreOptions? options = null)
    {
        ArgumentNullException.ThrowIfNull(wal); ArgumentNullException.ThrowIfNull(provider); options ??= new();
        (options.Limits ?? new()).Validate();
        if (options.MaxNamespaceEntries is < 2 or > 65536) throw new ArgumentOutOfRangeException(nameof(options));
        if (!wal.SupportsReplacement) throw new ArgumentException("LSM requires an atomic replaceable WAL.", nameof(wal));
        var store = new PhysicalLsmJournalStore(wal, provider, options);
        try { store.Run(() => { store.Recover(); return true; }); return store; }
        catch (Exception cause)
        {
            var errors = store.Release(); store._closed = true;
            if (errors.Count != 0) throw new AggregateException("LSM recovery and lease cleanup failed.", new[] { cause }.Concat(errors));
            throw;
        }
    }
    private T Run<T>(Func<T> action)
    {
        lock (_gate)
        {
            if (_closed) throw new ObjectDisposedException(nameof(PhysicalLsmJournalStore));
            if (_fenced) throw new InvalidOperationException("Uncertain LSM owner is fenced; reopen.");
            if (_busy) throw new InvalidOperationException("LSM reentry is forbidden.");
            _busy = true;
            try { T result = action(); _fenced = false; return result; }
            finally { _busy = false; }
        }
    }
    private T Effect<T>(Func<T> action) { _fenced = true; return action(); }
    private void Effect(Action action) { _fenced = true; action(); }
    private static InvalidDataException Invalid(string message) => new("Invalid physical LSM: " + message);
    private static long Cost(SortedJournalEntry row) => row.Key.Length + 8L + (row.Entries?.Sum(item => (long)Utf8.GetByteCount(item) + 1) ?? 0);
    private static Index Describe(SortedJournalEntry row, ulong owner) => new(row.Generation, row.Entries is null, Cost(row), owner);
    private SortedTableOptions TableOptions() => new(_limits.MaxKeys, 67_108_864);
    private void CheckDevice(IPageDevice device)
    { ArgumentNullException.ThrowIfNull(device); if (_tableDurable && !device.SupportsDurableFlush) throw Invalid("durable namespace returned a non-durable table device"); }
    private void Admit(IReadOnlyDictionary<string, Index> index)
    { if (index.Count > _limits.MaxKeys || index.Values.Sum(row => row.Bytes) > _limits.MaxBytes) throw Invalid("retained capacity exceeded"); }
    private SortedJournalEntry? Value(string key)
    {
        if (!_index.TryGetValue(key, out var item)) return null;
        if (item.Owner == 0) return _state.Buffer.Single(row => row.Key == key);
        var row = _tables[item.Owner].Get(key);
        if (row is null || row.Generation != item.Generation || (row.Entries is null) != item.Tombstone || Cost(row) != item.Bytes) throw Invalid("point lookup changed");
        return row;
    }
    private JournalVersion Version(string key)
    { var row = Value(key); return new(row?.Generation ?? 0, row?.Entries?.ToArray() ?? Array.Empty<string>()); }
    public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys) => Run(() =>
    { ArgumentNullException.ThrowIfNull(keys); ClusterEngine<uint>.ValidateKeys(keys); return keys.Select(Version).ToArray(); });
    private static bool Equal(IReadOnlyList<string>? left, IReadOnlyList<string>? right) => left is null || right is null ? left == right : left.SequenceEqual(right);
    public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates) => Run(() =>
    {
        ArgumentNullException.ThrowIfNull(keys); ArgumentNullException.ThrowIfNull(expected); ArgumentNullException.ThrowIfNull(updates);
        ClusterEngine<uint>.ValidateKeys(keys);
        if (keys.Count != expected.Count || keys.Count != updates.Count) throw new ArgumentException("LSM group array lengths.");
        var guards = expected.Select(item => (item ?? throw new ArgumentException("Null LSM guard.")).Generation).ToArray();
        var owned = updates.Select(items => items?.Select(item => { ArgumentNullException.ThrowIfNull(item); _ = Utf8.GetByteCount(item); return item; }).ToArray()).ToArray();
        for (int i = 0; i < keys.Count; i++) if ((_index.GetValueOrDefault(keys[i])?.Generation ?? 0) != guards[i]) return false;
        var changes = new List<SortedJournalEntry>();
        for (int i = 0; i < keys.Count; i++) if (owned[i] is { } entries && !Equal(Value(keys[i])?.Entries, entries)) changes.Add(new(keys[i], 0, entries));
        if (changes.Count != 0) Commit(changes); return true;
    });
    public bool CompareDelete(string key, ulong expectedGeneration) => Run(() =>
    {
        ClusterEngine<uint>.ValidateKeys(new[] { key }); if (expectedGeneration == 0) throw new ArgumentOutOfRangeException(nameof(expectedGeneration));
        if (!_index.TryGetValue(key, out var current) || current.Tombstone || current.Generation != expectedGeneration) return false;
        Commit(new[] { new SortedJournalEntry(key, 0, null) }); return true;
    });
    public IReadOnlyList<JournalNamespaceEntry> ScanNamespace(JournalScanOptions? options = null) => Run(() =>
    {
        options ??= new(); if (options.MaxKeys <= 0 || options.MaxBytes <= 0 || options.PageKeys is < 1 or > 256) throw new ArgumentOutOfRangeException(nameof(options));
        var result = new List<JournalNamespaceEntry>(); long bytes = 0;
        foreach (var (key, item) in _index) if (!item.Tombstone)
        {
            var value = options.KeysOnly ? new JournalVersion(item.Generation, Array.Empty<string>()) : Version(key);
            bytes += key.Length + 8L + value.Entries.Sum(entry => (long)Utf8.GetByteCount(entry) + 1);
            if (result.Count == options.MaxKeys || bytes > options.MaxBytes) throw Invalid("namespace scan admission"); result.Add(new(key, value));
        }
        return result.ToArray();
    });
    private List<Exception> Release()
    {
        var errors = new List<Exception>();
        foreach (var (id, device) in _leases.Reverse().ToArray()) try { _provider.Close(device); _leases.Remove(id); } catch (Exception cause) { errors.Add(cause); }
        return errors;
    }
    public void Dispose()
    {
        lock (_gate)
        {
            if (_closed && _leases.Count == 0) return;
            if (_busy) throw new InvalidOperationException("LSM reentry is forbidden."); _closed = true;
            var errors = Release(); if (errors.Count != 0) throw new AggregateException("LSM cleanup failed; ownership remains fenced.", errors);
        }
    }
}
