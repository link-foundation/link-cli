using System.Globalization;
namespace Foundation.Data.Doublets.Cli.Engine;

internal sealed class FileLsmFiles
{
    private sealed class Lease(FilePageDevice device, string name)
    { internal readonly FilePageDevice Device = device; internal readonly string Name = name; internal bool Closed; internal Exception? Failure; }
    private readonly string _directory;
    private readonly FileLsmOptions _options;
    private readonly FileLsmIo _io;
    private readonly FileStream _owner;
    private readonly List<NativeLsmReadOnly> _readers = new();
    private readonly List<Lease> _raw = new();
    private readonly HashSet<string> _owned = new(StringComparer.Ordinal);
    private readonly Dictionary<IPageDevice, Lease[]> _leases = new();
    private bool _closed, _uncertain;
    internal FileLsmFiles(string directory, FileLsmOptions options, FileLsmIo io)
    {
        _directory = Path.GetFullPath(directory); _options = options; _io = io;
        NativeLsmFileType.Check(_directory, directory: true); _owner = io.OpenOwner(PathOf(".lsm-writer"));
    }
    private string PathOf(string name) => Path.Combine(_directory, name);
    internal static string[] Parts(string name) => new[] { name, name + ".meta0", name + ".meta1" };
    private IEnumerable<string> WalNames()
    {
        yield return "wal"; if (_options.MirroredRoot) yield return "wal.mirror";
        foreach (int i in new[] { 0, 1 }) { yield return $"wal.bank{i}"; if (_options.MirroredBanks) yield return $"wal.bank{i}.mirror"; }
    }
    internal IReadOnlyList<ulong> List(int bound)
    {
        var permitted = WalNames().SelectMany(Parts).Append(".lsm-writer").ToHashSet(StringComparer.Ordinal);
        int maximum = permitted.Count + _options.StoreOptions.MaxNamespaceEntries * (_options.MirroredTables ? 6 : 3) + _owned.Count;
        var ids = new SortedSet<ulong>(); int count = 0; IEnumerator<string> entries;
        try { entries = _io.Enumerate(_directory).GetEnumerator(); } catch { _uncertain = true; throw; }
        var errors = new List<Exception>();
        try
        {
            while (entries.MoveNext())
            {
                if (++count > maximum) throw new InvalidDataException("Native LSM enumeration bound.");
                string path = entries.Current, name = Path.GetFileName(path); NativeLsmFileType.Check(path);
                if (permitted.Contains(name) || _owned.Contains(name)) continue;
                string table = name;
                if (table.EndsWith(".meta0", StringComparison.Ordinal) || table.EndsWith(".meta1", StringComparison.Ordinal)) table = table[..^6];
                if (table.EndsWith(".mirror", StringComparison.Ordinal))
                { if (!_options.MirroredTables) throw new InvalidDataException("Unexpected native LSM mirror."); table = table[..^7]; }
                if (!table.StartsWith("table-", StringComparison.Ordinal) || !ulong.TryParse(table.AsSpan(6), NumberStyles.None, CultureInfo.InvariantCulture, out ulong id)
                    || id == 0 || table != "table-" + id.ToString(CultureInfo.InvariantCulture)) throw new InvalidDataException("Foreign native LSM entry or owner.");
                ids.Add(id); if (ids.Count > _options.StoreOptions.MaxNamespaceEntries) throw new InvalidDataException("Native LSM namespace bound.");
            }
        }
        catch (Exception error) { errors.Add(error); }
        try { entries.Dispose(); } catch (Exception error) { _uncertain = true; errors.Add(error); }
        Throw(errors); if (ids.Count > bound) throw new InvalidDataException("Native LSM namespace bound."); return ids.ToArray();
    }
    private IRepairPageDevice Read(string name)
    { try { var device = new NativeLsmReadOnly(PathOf(name), _io); _readers.Add(device); return device; } catch { _uncertain = true; throw; } }
    private IRepairPageDevice Acquire(string name)
    {
        try { var device = _io.OpenDevice(PathOf(name)); _raw.Add(new(device, name)); _owned.Add(name + ".writer"); return device; }
        catch { _uncertain = true; throw; }
    }
    internal (IPageDevice Root, IPageDevice First, IPageDevice Second) Wal(bool readOnly)
    {
        Func<string, IRepairPageDevice> open = readOnly ? Read : Acquire;
        var primary = open("wal");
        IPageDevice root = _options.MirroredRoot ? new MirroredCheckpointRoot(primary, open("wal.mirror"), _options.MirroredBanks ? 1U : 0U) : primary;
        IPageDevice Bank(int i) { string name = $"wal.bank{i}"; var first = open(name); return _options.MirroredBanks ? new MirroredPageDevice(first, open(name + ".mirror")) : first; }
        return (root, Bank(0), Bank(1));
    }
    private IPageDevice Table(ulong id, Func<string, IRepairPageDevice> open)
    { string name = "table-" + id.ToString(CultureInfo.InvariantCulture); var first = open(name); return _options.MirroredTables ? new MirroredPageDevice(first, open(name + ".mirror")) : first; }
    private IPageDevice OpenTable(ulong id)
    { int before = _raw.Count; var device = Table(id, Acquire); _leases.Add(device, _raw.Skip(before).ToArray()); return device; }
    private IPageDevice CreateTable(ulong id)
    {
        foreach (string name in TableNames(id).SelectMany(Parts)) if (NativeLsmFileType.Check(PathOf(name), missing: true)) throw new IOException("Occupied native LSM table identity.");
        return OpenTable(id);
    }
    private IEnumerable<string> TableNames(ulong id)
    { string name = "table-" + id.ToString(CultureInfo.InvariantCulture); yield return name; if (_options.MirroredTables) yield return name + ".mirror"; }
    private void CloseLease(Lease lease)
    {
        if (lease.Closed) { if (lease.Failure is not null) throw lease.Failure; return; }
        lease.Closed = true;
        try { lease.Device.Dispose(); _owned.Remove(lease.Name + ".writer"); }
        catch (Exception error) { lease.Failure = error; _uncertain = true; throw; }
    }
    private void CloseTable(IPageDevice device)
    {
        var errors = new List<Exception>();
        foreach (var lease in _leases[device].Reverse()) try { CloseLease(lease); } catch (Exception error) { errors.Add(error); }
        Throw(errors); _leases.Remove(device); _raw.RemoveAll(lease => lease.Closed);
    }
    private void DeleteTable(ulong id)
    {
        foreach (string name in TableNames(id).SelectMany(Parts)) _io.Delete(PathOf(name));
        if (!OperatingSystem.IsWindows()) try { _io.SyncDirectory(_directory); } catch { _uncertain = true; throw; }
    }
    internal ILsmTableNamespace Provider(bool readOnly) => new Namespace(this, readOnly);
    private sealed class Namespace(FileLsmFiles files, bool readOnly) : ILsmTableNamespace
    {
        public bool SupportsDurableFlush => !readOnly && !OperatingSystem.IsWindows();
        public IReadOnlyList<ulong> List(int maxEntries) => files.List(maxEntries);
        public IPageDevice Open(ulong id) => readOnly ? new NativeLsmDeferred(files.Table(id, files.Read)) : files.OpenTable(id);
        public IPageDevice Create(ulong id) => readOnly ? throw new InvalidOperationException("Read-only native LSM create.") : files.CreateTable(id);
        public void Close(IPageDevice device) { if (!readOnly) files.CloseTable(device); }
        public void Delete(ulong id) { if (!readOnly) files.DeleteTable(id); }
    }
    internal void CloseReaders()
    {
        var errors = new List<Exception>(); foreach (var reader in _readers.AsEnumerable().Reverse()) try { reader.Dispose(); } catch (Exception error) { errors.Add(error); }
        _readers.Clear(); if (errors.Count != 0) _uncertain = true; Throw(errors);
    }
    internal void Close(List<Exception>? errors = null)
    {
        if (_closed) { if (_uncertain) throw new InvalidOperationException("Native LSM remains fenced."); return; }
        _closed = true; errors ??= new();
        try { CloseReaders(); } catch (Exception error) { errors.Add(error); }
        foreach (var lease in _raw.AsEnumerable().Reverse()) try { CloseLease(lease); } catch (Exception error) { errors.Add(error); }
        try { _io.Close(_owner); } catch (Exception error) { _uncertain = true; errors.Add(error); }
        if (errors.Count == 0 && !_uncertain) try { _io.Delete(PathOf(".lsm-writer")); } catch (Exception error) { _uncertain = true; errors.Add(error); }
        if (errors.Count != 0) _uncertain = true;
        if (_uncertain && errors.Count == 0) errors.Add(new InvalidOperationException("Native LSM ownership remains uncertain.")); Throw(errors);
    }
    private static void Throw(List<Exception> errors) { if (errors.Count != 0) throw new AggregateException("Native LSM discovery or cleanup failed.", errors); }
}
