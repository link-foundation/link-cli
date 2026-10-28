namespace Foundation.Data.Doublets.Cli.Engine;

/// <summary>Owns native devices at path and path + '.mirror'. Acquires both
/// exclusive owner files; never automatically removes stale ownership.</summary>
public sealed class FileMirroredPageDevice : MirroredPageDevice, IDisposable
{
    private readonly FilePageDevice[] _owned;
    private bool _closed;
    public FileMirroredPageDevice(string path) : this(path, name => new FilePageDevice(name)) { }
    internal FileMirroredPageDevice(string path, Func<string, FilePageDevice> open) : this(OpenBoth(path, open)) { }
    private FileMirroredPageDevice(FilePageDevice[] devices) : base(devices[0], devices[1]) { _owned = devices; }
    private static FilePageDevice[] OpenBoth(string path, Func<string, FilePageDevice> open)
    {
        var first = open(path);
        try { return new[] { first, open(path + ".mirror") }; }
        catch (Exception cause)
        {
            try { first.Dispose(); } catch (Exception cleanup) { throw new AggregateException("Mirror initialization and cleanup failed.", cause, cleanup); }
            throw;
        }
    }
    public void Dispose()
    {
        if (_closed) return; _closed = true; Fence(); var errors = new List<Exception>();
        foreach (var device in _owned.Reverse()) { try { device.Dispose(); } catch (Exception error) { errors.Add(error); } }
        if (errors.Count > 0) throw new AggregateException("Mirror cleanup failed; uncertain owners remain fenced.", errors);
    }
}
