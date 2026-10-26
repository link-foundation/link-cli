using static Foundation.Data.Doublets.Cli.Engine.PageFormat;

namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Owns a raw root and fixed .bank0/.bank1 devices, with optional
/// .mirror copies of each bank. Acquires every fence before validation/repair.
/// Close never flushes pending work; it attempts every device, root last.
/// Uncertain native cleanup retains the affected .writer fences.</summary>
public sealed class FileCheckpointPagedCommitLog : IReplaceableCommitLog, IDisposable
{
    private readonly FilePageDevice[] _owned;
    private CheckpointPagedCommitLog? _inner;
    public FileCheckpointPagedCommitLog(string path, bool mirroredBanks = false, ulong maxPages = MaxPages)
        : this(path, mirroredBanks, maxPages, name => new FilePageDevice(name)) { }

    internal FileCheckpointPagedCommitLog(string path, bool mirroredBanks, ulong maxPages, Func<string, FilePageDevice> open)
    {
        path = Path.GetFullPath(path);
        var acquired = new List<FilePageDevice>();
        FilePageDevice Acquire(string name) { var device = open(name); acquired.Add(device); return device; }
        try
        {
            var root = Acquire(path);
            IPageDevice Bank(int index)
            {
                string name = path + $".bank{index}";
                var primary = Acquire(name);
                return mirroredBanks ? new MirroredPageDevice(primary, Acquire(name + ".mirror")) : primary;
            }
            var first = Bank(0); var second = Bank(1);
            _inner = new CheckpointPagedCommitLog(root, first, second, maxPages);
            _owned = acquired.ToArray();
        }
        catch (Exception cause)
        {
            var cleanup = CloseAll(acquired);
            if (cleanup.Count > 0) throw new AggregateException("Native checkpoint initialization and cleanup failed.", new[] { cause }.Concat(cleanup));
            throw;
        }
    }
    private CheckpointPagedCommitLog Inner => _inner ?? throw new ObjectDisposedException(nameof(FileCheckpointPagedCommitLog));
    public ulong Epoch => Inner.Epoch;
    public bool SupportsDurableFlush => true;
    public bool SupportsReplacement => true;
    public IReadOnlyList<string> Read() => Inner.Read();
    public void Append(string record) => Inner.Append(record);
    public void Flush() => Inner.Flush();
    public bool Replace(IReadOnlyList<string> records) => Inner.Replace(records);
    private static List<Exception> CloseAll(IEnumerable<FilePageDevice> devices)
    {
        var errors = new List<Exception>();
        foreach (var device in devices.Reverse()) { try { device.Dispose(); } catch (Exception error) { errors.Add(error); } }
        return errors;
    }
    public void Dispose()
    {
        if (_inner is null) return;
        _inner = null;
        var errors = CloseAll(_owned);
        if (errors.Count > 0) throw new AggregateException("Native checkpoint cleanup failed; uncertain owners remain fenced.", errors);
    }
}
