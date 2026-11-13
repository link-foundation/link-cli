using System.Diagnostics.CodeAnalysis;
using System.Diagnostics;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>Reads a blocking source on a thread of its own, so reads can time out and hanging up never waits for it.</summary>
internal sealed class ThreadedReader
{
    private const int Buffered = 1 << 20;
    private static readonly TimeSpan Slice = TimeSpan.FromMilliseconds(100);

    private readonly object _lock = new();
    private readonly Queue<byte[]> _chunks = new();
    private int _offset;
    private int _length;
    private bool _ended;
    private IpcException? _error;
    private bool _discarding;

    private ThreadedReader() { }

    public static ThreadedReader Start(string name, Stream source)
    {
        var reader = new ThreadedReader();
        new Thread(() => reader.Run(source)) { IsBackground = true, Name = name }.Start();
        return reader;
    }

    private void Run(Stream source)
    {
        var chunk = new byte[64 * 1024];
        while (true)
        {
            int count;
            try
            {
                count = source.Read(chunk);
            }
            catch (Exception error) when (error is IOException or ObjectDisposedException)
            {
                lock (_lock)
                {
                    if (error is ObjectDisposedException)
                    {
                        _ended = true;
                    }
                    else
                    {
                        _error = IpcTransports.Wrap(error);
                    }
                    Monitor.PulseAll(_lock);
                }
                return;
            }
            lock (_lock)
            {
                if (count == 0)
                {
                    _ended = true;
                    Monitor.PulseAll(_lock);
                    return;
                }
                while (!_discarding && _length >= Buffered)
                {
                    Monitor.Wait(_lock);
                }
                if (!_discarding)
                {
                    _chunks.Enqueue(chunk.AsSpan(0, count).ToArray());
                    _length += count;
                }
                Monitor.PulseAll(_lock);
            }
        }
    }

    public int Read(Span<byte> buffer, TimeSpan? timeout, Func<bool> closed)
    {
        var deadline = Deadline.After(timeout);
        lock (_lock)
        {
            while (true)
            {
                if (_length > 0)
                {
                    var copied = 0;
                    while (copied < buffer.Length && _chunks.TryPeek(out var head))
                    {
                        var count = Math.Min(buffer.Length - copied, head.Length - _offset);
                        head.AsSpan(_offset, count).CopyTo(buffer[copied..]);
                        copied += count;
                        _offset += count;
                        if (_offset == head.Length)
                        {
                            _chunks.Dequeue();
                            _offset = 0;
                        }
                    }
                    _length -= copied;
                    Monitor.PulseAll(_lock);
                    return copied;
                }
                if (_ended || closed())
                {
                    return 0;
                }
                if (_error is not null)
                {
                    throw new IpcException(_error.Kind, _error.Message);
                }
                var wait = Slice;
                if (deadline is { } limit)
                {
                    if (limit.Passed)
                    {
                        throw IpcException.TimedOut();
                    }
                    if (limit.Remaining < wait)
                    {
                        wait = limit.Remaining;
                    }
                }
                Monitor.Wait(_lock, wait);
            }
        }
    }

    /// <summary>Stops buffering; later input is thrown away.</summary>
    public void Discard()
    {
        lock (_lock)
        {
            _discarding = true;
            _chunks.Clear();
            _offset = 0;
            _length = 0;
            Monitor.PulseAll(_lock);
        }
    }
}

/// <summary>Anonymous pipes: <c>stdio</c> (this process's standard input and output) and <c>exec:COMMAND</c>.</summary>
internal static class StdioTransports
{
    /// <summary>Signals the end of the single <c>stdio</c> connection.</summary>
    private sealed class Ended
    {
        private readonly object _lock = new();
        private bool _ended;

        public void Set()
        {
            lock (_lock)
            {
                _ended = true;
                Monitor.PulseAll(_lock);
            }
        }

        public void Wake()
        {
            lock (_lock)
            {
                Monitor.PulseAll(_lock);
            }
        }

        public void WaitUntil(Func<bool> closed)
        {
            lock (_lock)
            {
                while (!_ended && !closed())
                {
                    Monitor.Wait(_lock, TimeSpan.FromMilliseconds(100));
                }
            }
        }
    }

    private sealed class StdioTransport(ThreadedReader input, Stream output, Ended ended) : ITransport, IDisposable
    {
        private readonly Lock _outputLock = new();
        private Stream? _output = output;
        private volatile bool _closed;

        public string Kind => "stdio";

        public int Read(Span<byte> buffer, TimeSpan? timeout) => input.Read(buffer, timeout, () => _closed);

        public int Write(ReadOnlySpan<byte> data)
        {
            lock (_outputLock)
            {
                if (_output is null)
                {
                    throw new IpcException(IpcErrorKind.BrokenPipe, "standard output was closed");
                }
                try
                {
                    _output.Write(data);
                }
                catch (IOException error)
                {
                    throw IpcTransports.Wrap(error);
                }
                return data.Length;
            }
        }

        public void Flush()
        {
            lock (_outputLock)
            {
                _output?.Flush();
            }
        }

        public void CloseWrite()
        {
            lock (_outputLock)
            {
                if (_output is null)
                {
                    return;
                }
                _output.Flush();
                _output.Dispose();
                _output = null;
                // The reader of our standard output sees its end only when the descriptor itself is closed.
                if (OperatingSystem.IsWindows())
                {
                    WindowsNative.CloseStdout();
                }
                else
                {
                    PosixNative.CloseStdout();
                }
            }
        }

        public void Shutdown()
        {
            if (_closed)
            {
                return;
            }
            _closed = true;
            IpcTransports.Trace("stdio connection ended");
            try
            {
                CloseWrite();
            }
            catch (Exception error) when (error is IOException or UnauthorizedAccessException)
            {
            }
            input.Discard();
            ended.Set();
        }

        public void Dispose() => Shutdown();
    }

    private sealed class StdioListener(string endpoint) : IIpcListener
    {
        private readonly Ended _ended = new();
        private bool _taken;
        private volatile bool _closed;

        [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
        public IpcStream? Accept()
        {
            if (_closed)
            {
                return null;
            }
            if (!_taken)
            {
                _taken = true;
                var transport = new StdioTransport(ThreadedReader.Start("clink-stdin", Console.OpenStandardInput()),
                    Console.OpenStandardOutput(), _ended);
                IpcTransports.Trace("stdio connection started");
                return new IpcStream(transport);
            }
            // Only one connection: wait for it to end, then stop.
            _ended.WaitUntil(() => _closed);
            return null;
        }

        public string Endpoint => endpoint;

        public Action Closer() => () =>
        {
            _closed = true;
            _ended.Wake();
        };

        public void Dispose() => _closed = true;
    }

    public static IIpcListener ListenStdio(Endpoint endpoint) => new StdioListener("stdio");

    /// <summary>The argument vector of <c>exec:COMMAND</c>: a JSON array of strings, or words separated by whitespace.</summary>
    public static List<string> CommandLine(string address)
    {
        address = address.Trim();
        var argv = address.StartsWith('[')
            ? IpcJson.ParseStrings(address)
            : [.. address.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries)];
        if (argv.Count == 0 || argv[0].Length == 0)
        {
            throw IpcException.Invalid("`exec:` needs a command");
        }
        return argv;
    }

    private sealed class ExecTransport(Process child, ThreadedReader output, string program) : ITransport, IDisposable
    {
        private readonly Lock _inputLock = new();
        private Stream? _input = child.StandardInput.BaseStream;
        private volatile bool _closed;

        public string Kind => "exec";

        public int Read(Span<byte> buffer, TimeSpan? timeout) => output.Read(buffer, timeout, () => _closed);

        public int Write(ReadOnlySpan<byte> data)
        {
            lock (_inputLock)
            {
                if (_input is null)
                {
                    throw new IpcException(IpcErrorKind.BrokenPipe, "the outgoing direction was closed");
                }
                try
                {
                    _input.Write(data);
                }
                catch (IOException error)
                {
                    throw IpcTransports.Wrap(error);
                }
                return data.Length;
            }
        }

        public void Flush()
        {
            lock (_inputLock)
            {
                try
                {
                    _input?.Flush();
                }
                catch (IOException error)
                {
                    throw IpcTransports.Wrap(error);
                }
            }
        }

        public void CloseWrite()
        {
            lock (_inputLock)
            {
                try
                {
                    _input?.Dispose();
                }
                catch (IOException)
                {
                }
                _input = null;
            }
        }

        public void Shutdown()
        {
            if (_closed)
            {
                return;
            }
            _closed = true;
            CloseWrite();
            output.Discard();
            if (child.WaitForExit(TimeSpan.FromSeconds(5)))
            {
                IpcTransports.Trace($"exec `{program}` exited: exit status: {child.ExitCode}");
            }
            else
            {
                IpcTransports.Trace($"exec `{program}` killed after 5 s");
                try
                {
                    child.Kill(true);
                    child.WaitForExit();
                }
                catch (Exception error) when (error is InvalidOperationException or System.ComponentModel.Win32Exception)
                {
                }
            }
        }

        public void Dispose()
        {
            Shutdown();
            child.Dispose();
        }
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IpcStream ConnectExec(Endpoint endpoint)
    {
        var argv = CommandLine(endpoint.Address);
        var start = new ProcessStartInfo(argv[0])
        {
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = false,
            UseShellExecute = false,
        };
        foreach (var argument in argv.Skip(1))
        {
            start.ArgumentList.Add(argument);
        }
        Process child;
        try
        {
            child = Process.Start(start) ?? throw new IpcException(IpcErrorKind.Other, $"cannot start `{argv[0]}`");
        }
        catch (System.ComponentModel.Win32Exception error)
        {
            var kind = error.NativeErrorCode == 2 ? IpcErrorKind.NotFound : IpcErrorKind.Other;
            throw new IpcException(kind, $"cannot start `{argv[0]}`: {error.Message}", error);
        }
        IpcTransports.Trace($"exec started `{argv[0]}` as process {child.Id}");
        var output = ThreadedReader.Start("clink-exec-stdout", child.StandardOutput.BaseStream);
        return new IpcStream(new ExecTransport(child, output, argv[0]));
    }
}
