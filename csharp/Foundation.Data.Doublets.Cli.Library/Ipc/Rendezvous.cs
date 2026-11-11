using System.Diagnostics;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>Sleeps with a backoff from 1 ms up to a limit.</summary>
internal sealed class Backoff
{
    private static readonly TimeSpan Floor = TimeSpan.FromTicks(500); // 50 µs
    private readonly TimeSpan _limit;
    private TimeSpan _current;

    public Backoff(TimeSpan limit)
    {
        _limit = limit < Floor ? Floor : limit;
        _current = TimeSpan.FromMilliseconds(1) < _limit ? TimeSpan.FromMilliseconds(1) : _limit;
    }

    public void Sleep()
    {
        Thread.Sleep(_current);
        _current = _current * 2 < _limit ? _current * 2 : _limit;
    }
}

/// <summary>The rendezvous directory where local kinds meet (see "Rendezvous directory" in the spec).</summary>
internal static class RendezvousFiles
{
    public const ulong Version = 1;
    public const string ListenerFile = "listener.json";

    /// <summary>File suffixes this protocol creates in a rendezvous directory.</summary>
    public static readonly string[] OwnedSuffixes = [".req", ".ack", ".tmp", ".c2s", ".s2c", ".mmap", ".ring", ".sig"];

    public static void WriteAtomically(string path, string text)
    {
        var temporary = path + ".tmp";
        File.WriteAllText(temporary, text);
        try
        {
            File.Move(temporary, path, overwrite: true);
        }
        catch
        {
            TryDelete(temporary);
            throw;
        }
    }

    public static IpcJson ReadObject(string path) => IpcJson.Parse(File.ReadAllText(path));

    public static void TryDelete(string path)
    {
        try
        {
            File.Delete(path);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
        }
    }

    public static void CreateDirectory(string directory)
    {
        if (OperatingSystem.IsWindows())
        {
            Directory.CreateDirectory(directory);
        }
        else
        {
            Directory.CreateDirectory(directory, UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute);
        }
    }

    public static string CanonicalKind(string kind) => kind == "pipe" ? "fifo" : kind;

    public static bool ValidId(string id) => id.Length == 20 && id.All(c => c is >= '0' and <= '9' or >= 'a' and <= 'f');

    public static IpcException Refused(string message) => new(IpcErrorKind.ConnectionRefused, message);
}

/// <summary>The listening side of a rendezvous directory.</summary>
internal sealed class RendezvousListener : IDisposable
{
    private readonly string _directory;
    private readonly string _kind;
    private readonly TimeSpan _acceptPoll;
    private volatile bool _closed;
    private int _disposed;

    public RendezvousListener(Endpoint endpoint, string kind)
    {
        _directory = endpoint.Address;
        _kind = kind;
        _acceptPoll = endpoint.Millis("accept-poll", 20);
        Endpoint = endpoint.ToString();
        RendezvousFiles.CreateDirectory(_directory);
        var listener = Path.Combine(_directory, RendezvousFiles.ListenerFile);
        try
        {
            if (RendezvousFiles.ReadObject(listener).Number("pid") is { } pid && IpcTransports.ProcessAlive((uint)pid))
            {
                throw new IpcException(IpcErrorKind.AddrInUse, $"process {pid} already listens on `{_directory}`");
            }
        }
        catch (Exception error) when (error is IOException and not IpcException { Kind: IpcErrorKind.AddrInUse }
            or UnauthorizedAccessException)
        {
        }
        // A stale directory: clear what this protocol left behind.
        try
        {
            foreach (var path in Directory.EnumerateFiles(_directory))
            {
                var name = Path.GetFileName(path);
                if (name == RendezvousFiles.ListenerFile || RendezvousFiles.OwnedSuffixes.Any(name.EndsWith))
                {
                    RendezvousFiles.TryDelete(path);
                }
            }
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
        }
        RendezvousFiles.WriteAtomically(listener, new IpcJson()
            .With("v", RendezvousFiles.Version)
            .With("kind", RendezvousFiles.CanonicalKind(kind))
            .With("pid", IpcTransports.ProcessId)
            .ToJson());
    }

    public string Endpoint { get; }

    public bool IsClosed => _closed;

    public Action Closer() => () => _closed = true;

    /// <summary>Waits for the next request; null once closed.</summary>
    public RendezvousRequest? Next()
    {
        var backoff = new Backoff(_acceptPoll);
        while (true)
        {
            if (_closed)
            {
                return null;
            }
            var names = Directory.EnumerateFiles(_directory, "*.req")
                .Select(Path.GetFileName)
                .OfType<string>()
                .Where(name => name.EndsWith(".req", StringComparison.Ordinal))
                .Order(StringComparer.Ordinal)
                .ToList();
            foreach (var name in names)
            {
                var path = Path.Combine(_directory, name);
                string text;
                try
                {
                    text = File.ReadAllText(path);
                    File.Delete(path);
                }
                catch (Exception error) when (error is IOException or UnauthorizedAccessException)
                {
                    continue;
                }
                var request = new RendezvousRequest(name[..^".req".Length], _directory);
                IpcJson? fields = null;
                try
                {
                    fields = IpcJson.Parse(text);
                }
                catch (IpcException)
                {
                }
                if (fields is not null
                    && fields.Number("v") == RendezvousFiles.Version
                    && fields.Text("kind") == RendezvousFiles.CanonicalKind(_kind)
                    && fields.Number("pid") is not null
                    && RendezvousFiles.ValidId(request.Id))
                {
                    request.Fields = fields;
                    IpcTransports.Trace($"{_kind} request {request.Id} from process {request.Pid}");
                    return request;
                }
                IpcTransports.Trace($"{_kind} refused malformed request {name}");
                request.Refuse("malformed or mismatched request");
            }
            backoff.Sleep();
        }
    }

    /// <summary>Answers requests with <paramref name="open"/> until one succeeds.</summary>
    public IpcStream? AcceptWith(string kind, Func<RendezvousRequest, (IpcJson Fields, IpcStream Stream)> open)
    {
        while (Next() is { } request)
        {
            try
            {
                (IpcJson Fields, IpcStream Stream) opened;
                try
                {
                    opened = open(request);
                }
                catch (Exception error) when (error is IOException or UnauthorizedAccessException)
                {
                    IpcTransports.Trace($"{kind} refused {request.Id}: {error.Message}");
                    request.Refuse(error.Message);
                    continue;
                }
                try
                {
                    request.Ack(opened.Fields);
                }
                catch
                {
                    opened.Stream.Dispose();
                    throw;
                }
                IpcTransports.Trace($"{kind} accepted {request.Id} from process {request.Pid}");
                return opened.Stream;
            }
            finally
            {
                request.RefuseUnanswered();
            }
        }
        return null;
    }

    public void Dispose()
    {
        if (Interlocked.Exchange(ref _disposed, 1) != 0)
        {
            return;
        }
        _closed = true;
        var listener = Path.Combine(_directory, RendezvousFiles.ListenerFile);
        try
        {
            if (RendezvousFiles.ReadObject(listener).Number("pid") == IpcTransports.ProcessId)
            {
                File.Delete(listener);
            }
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
        }
        try
        {
            Directory.Delete(_directory);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
        }
    }
}

/// <summary>A request claimed by the listener; answer it with <see cref="Ack"/> or <see cref="Refuse"/>.</summary>
internal sealed class RendezvousRequest(string id, string directory)
{
    private bool _answered;

    public string Id { get; } = id;

    public IpcJson Fields { get; set; } = new();

    public uint Pid => (uint)(Fields.Number("pid") ?? 0);

    public string PathOf(string suffix) => Path.Combine(directory, $"{Id}.{suffix}");

    public void Ack(IpcJson fields)
    {
        _answered = true;
        RendezvousFiles.WriteAtomically(PathOf("ack"), fields
            .With("v", RendezvousFiles.Version)
            .With("pid", IpcTransports.ProcessId)
            .ToJson());
    }

    public void Refuse(string error)
    {
        _answered = true;
        try
        {
            RendezvousFiles.WriteAtomically(PathOf("ack"), new IpcJson()
                .With("v", RendezvousFiles.Version)
                .With("error", error)
                .ToJson());
        }
        catch (Exception failure) when (failure is IOException or UnauthorizedAccessException)
        {
        }
    }

    /// <summary>Refuses the request unless it was answered.</summary>
    public void RefuseUnanswered()
    {
        if (!_answered)
        {
            Refuse("the listener could not set the connection up");
        }
    }
}

/// <summary>The connecting side of a rendezvous directory.</summary>
internal sealed class Rendezvous
{
    private readonly string _directory;
    private readonly string _kind;
    private readonly Deadline _deadline;
    private readonly TimeSpan _poll;

    public Rendezvous(Endpoint endpoint, string kind)
    {
        _directory = endpoint.Address;
        _kind = kind;
        IpcJson listener;
        try
        {
            listener = RendezvousFiles.ReadObject(Path.Combine(_directory, RendezvousFiles.ListenerFile));
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            throw RendezvousFiles.Refused($"nobody listens on `{_directory}`: {error.Message}");
        }
        var pid = (uint)(listener.Number("pid") ?? 0);
        if (listener.Number("v") != RendezvousFiles.Version)
        {
            throw RendezvousFiles.Refused($"`{_directory}` uses another rendezvous version");
        }
        var found = listener.Text("kind") ?? "";
        if (found != RendezvousFiles.CanonicalKind(kind))
        {
            throw RendezvousFiles.Refused($"`{_directory}` listens for `{found}:` connections, not `{kind}:`");
        }
        if (pid == 0 || !IpcTransports.ProcessAlive(pid))
        {
            throw RendezvousFiles.Refused($"the listener of `{_directory}` (process {pid}) is gone");
        }
        Id = IpcTransports.RandomId();
        ServerPid = pid;
        _deadline = Deadline.After(endpoint.Timeout);
        _poll = endpoint.Millis("poll", 5);
    }

    public string Id { get; }

    public uint ServerPid { get; }

    public string PathOf(string suffix) => Path.Combine(_directory, $"{Id}.{suffix}");

    /// <summary>Writes the request and waits for the ack.</summary>
    public IpcJson Request(IpcJson fields)
    {
        var request = PathOf("req");
        var ack = PathOf("ack");
        RendezvousFiles.WriteAtomically(request, fields
            .With("v", RendezvousFiles.Version)
            .With("kind", RendezvousFiles.CanonicalKind(_kind))
            .With("pid", IpcTransports.ProcessId)
            .ToJson());
        IpcTransports.Trace($"{_kind} request {Id} written");
        var backoff = new Backoff(_poll < TimeSpan.FromMilliseconds(1) ? TimeSpan.FromMilliseconds(1) : _poll);
        var lastCheck = Stopwatch.GetTimestamp();
        while (true)
        {
            string? text = null;
            try
            {
                text = File.ReadAllText(ack);
            }
            catch (Exception error) when (error is IOException or UnauthorizedAccessException)
            {
            }
            if (!string.IsNullOrEmpty(text))
            {
                RendezvousFiles.TryDelete(ack);
                var answer = IpcJson.Parse(text);
                if (answer.Text("error") is { } refusal)
                {
                    throw RendezvousFiles.Refused($"the listener refused: {refusal}");
                }
                return answer;
            }
            var gone = false;
            if (Stopwatch.GetElapsedTime(lastCheck) >= TimeSpan.FromMilliseconds(100))
            {
                lastCheck = Stopwatch.GetTimestamp();
                gone = !IpcTransports.ProcessAlive(ServerPid);
            }
            if (gone || _deadline.Passed)
            {
                RendezvousFiles.TryDelete(request);
                RendezvousFiles.TryDelete(ack);
                throw gone
                    ? RendezvousFiles.Refused($"the listener (process {ServerPid}) is gone")
                    : new IpcException(IpcErrorKind.TimedOut, $"no answer from the listener of `{_directory}` in time");
            }
            backoff.Sleep();
        }
    }
}
