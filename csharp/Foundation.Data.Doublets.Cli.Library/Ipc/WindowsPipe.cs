using System.Diagnostics.CodeAnalysis;
using System.IO.Pipes;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using Microsoft.Win32.SafeHandles;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>
/// Windows named pipes (<c>pipe:NAME</c>): byte-mode, overlapped, local clients only. A named pipe cannot half-close,
/// so ending the outgoing direction closes the handle, as in the Rust implementation.
/// </summary>
[SupportedOSPlatform("windows")]
internal static partial class WindowsPipe
{
    private const uint OpenExisting = 3;
    private const uint FileFlagOverlapped = 0x40000000;
    private const uint SecuritySqosPresent = 0x00100000;
    private const uint SecurityIdentification = 1 << 16;
    private const int ErrorFileNotFound = 2;
    private const int ErrorPipeBusy = 231;
    private const int BufferSize = 64 * 1024;
    private static readonly TimeSpan Slice = TimeSpan.FromMilliseconds(100);

    [DllImport("kernel32.dll", EntryPoint = "CreateFileW", SetLastError = true, CharSet = CharSet.Unicode, ExactSpelling = true)]
    private static extern SafePipeHandle CreateFile(string name, uint access, uint share, nint security, uint disposition,
        uint flags, nint template);

    [DllImport("kernel32.dll", EntryPoint = "WaitNamedPipeW", SetLastError = true, CharSet = CharSet.Unicode, ExactSpelling = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool WaitNamedPipe(string name, uint milliseconds);

    /// <summary>The full pipe path of an address: a bare name lives under <c>\\.\pipe\</c>.</summary>
    public static string PipeName(string address)
    {
        if (address.Length == 0)
        {
            throw IpcException.Invalid("`pipe:` needs a name");
        }
        return address.StartsWith(@"\\", StringComparison.Ordinal) ? address : $@"\\.\pipe\{address}";
    }

    /// <summary>The name <see cref="NamedPipeServerStream"/> expects: the path without <c>\\.\pipe\</c>.</summary>
    private static string LocalName(string full)
    {
        const string Local = @"\\.\pipe\";
        if (!full.StartsWith(Local, StringComparison.OrdinalIgnoreCase))
        {
            throw IpcException.Invalid($"a server can only listen on a local pipe, not on `{full}`");
        }
        return full[Local.Length..];
    }

    /// <summary>One end of a pipe instance.</summary>
    private sealed class PipeTransport(PipeStream pipe) : ITransport, IDisposable
    {
        private readonly Lock _lock = new();
        private readonly CancellationTokenSource _closed = new();
        private readonly byte[] _readBuffer = new byte[BufferSize];
        private readonly SemaphoreSlim _reading = new(1, 1);
        private PipeStream? _pipe = pipe;
        private int _shutdown;

        public string Kind => "pipe";

        private PipeStream? Current
        {
            get
            {
                lock (_lock)
                {
                    return _pipe;
                }
            }
        }

        public int Read(Span<byte> buffer, TimeSpan? timeout)
        {
            if (buffer.IsEmpty)
            {
                return 0;
            }
            _reading.Wait();
            try
            {
                if (Current is not { } pipe)
                {
                    return 0;
                }
                using var limit = CancellationTokenSource.CreateLinkedTokenSource(_closed.Token);
                if (timeout is { } span)
                {
                    limit.CancelAfter(span);
                }
                int count;
                try
                {
                    count = pipe.ReadAsync(_readBuffer.AsMemory(0, Math.Min(buffer.Length, _readBuffer.Length)), limit.Token)
                        .AsTask().GetAwaiter().GetResult();
                }
                catch (OperationCanceledException) when (!_closed.IsCancellationRequested)
                {
                    throw IpcException.TimedOut();
                }
                catch (Exception error) when (error is OperationCanceledException or ObjectDisposedException or IOException)
                {
                    // A broken, closed or cancelled pipe ends the incoming direction.
                    return 0;
                }
                _readBuffer.AsSpan(0, count).CopyTo(buffer);
                return count;
            }
            finally
            {
                _reading.Release();
            }
        }

        public int Write(ReadOnlySpan<byte> data)
        {
            if (data.IsEmpty)
            {
                return 0;
            }
            if (Current is not { } pipe)
            {
                throw IpcException.BrokenPipe();
            }
            try
            {
                pipe.Write(data);
            }
            catch (Exception error) when (error is IOException or ObjectDisposedException or InvalidOperationException)
            {
                throw IpcException.BrokenPipe();
            }
            return data.Length;
        }

        private void Close()
        {
            PipeStream? pipe;
            lock (_lock)
            {
                pipe = _pipe;
                _pipe = null;
            }
            if (pipe is null)
            {
                return;
            }
            // Whatever was written stays readable by the peer.
            pipe.Dispose();
            IpcTransports.Trace("pipe closed");
        }

        public void CloseWrite() => Close();

        public void Shutdown()
        {
            if (Interlocked.Exchange(ref _shutdown, 1) != 0)
            {
                return;
            }
            _closed.Cancel();
            Close();
        }

        public void Dispose()
        {
            Shutdown();
        }
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IpcStream Connect(Endpoint endpoint)
    {
        var name = PipeName(endpoint.Address);
        var deadline = Deadline.After(endpoint.Timeout);
        while (true)
        {
            // The server may only identify this client, never impersonate it.
            var handle = CreateFile(name, 0x80000000 | 0x40000000, 0, 0, OpenExisting,
                FileFlagOverlapped | SecuritySqosPresent | SecurityIdentification, 0);
            if (!handle.IsInvalid)
            {
                try
                {
                    var stream = new NamedPipeClientStream(PipeDirection.InOut, true, true, handle);
                    IpcTransports.Trace($"pipe connected to {name}");
                    return new IpcStream(new PipeTransport(stream));
                }
                catch
                {
                    handle.Dispose();
                    throw;
                }
            }
            var code = Marshal.GetLastPInvokeError();
            handle.Dispose();
            switch (code)
            {
                case ErrorPipeBusy:
                    if (deadline.Passed)
                    {
                        throw IpcException.TimedOut();
                    }
                    var wait = deadline.Remaining < Slice * 10 ? deadline.Remaining : Slice * 10;
                    _ = WaitNamedPipe(name, (uint)Math.Max(1, Math.Ceiling(wait.TotalMilliseconds)));
                    break;
                case ErrorFileNotFound:
                    throw new IpcException(IpcErrorKind.ConnectionRefused, $"nobody listens on `{name}`");
                default:
                    throw WindowsNative.CodeError(code);
            }
        }
    }

    private sealed class PipeListener : IIpcListener
    {
        private readonly string _address;
        private readonly string _name;
        private readonly CancellationTokenSource _closed = new();
        private NamedPipeServerStream? _next;

        public PipeListener(string address, string name, NamedPipeServerStream first)
        {
            _address = address;
            _name = name;
            _next = first;
        }

        public static NamedPipeServerStream CreateInstance(string name, bool first) =>
            new(name, PipeDirection.InOut, NamedPipeServerStream.MaxAllowedServerInstances, PipeTransmissionMode.Byte,
                PipeOptions.Asynchronous | (first ? PipeOptions.FirstPipeInstance : PipeOptions.None), BufferSize, BufferSize);

        [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
        public IpcStream? Accept()
        {
            while (!_closed.IsCancellationRequested)
            {
                var instance = Interlocked.Exchange(ref _next, null) ?? CreateInstance(_name, false);
                try
                {
                    instance.WaitForConnectionAsync(_closed.Token).GetAwaiter().GetResult();
                }
                catch (OperationCanceledException)
                {
                    instance.Dispose();
                    return null;
                }
                catch (IOException error)
                {
                    // The client came and went before the connection completed.
                    IpcTransports.Trace($"pipe lost a client on {_address}: {error.Message}");
                    instance.Dispose();
                    continue;
                }
                if (_closed.IsCancellationRequested)
                {
                    instance.Dispose();
                    return null;
                }
                // The next client finds an instance waiting.
                try
                {
                    _next = CreateInstance(_name, false);
                }
                catch (IOException)
                {
                }
                IpcTransports.Trace($"pipe accepted on {_address}");
                return new IpcStream(new PipeTransport(instance));
            }
            return null;
        }

        public string Endpoint => $"pipe:{_address}";

        public Action Closer() => () =>
        {
            try
            {
                _closed.Cancel();
            }
            catch (ObjectDisposedException)
            {
            }
        };

        public void Dispose()
        {
            Closer()();
            Interlocked.Exchange(ref _next, null)?.Dispose();
        }
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IIpcListener Listen(Endpoint endpoint)
    {
        var full = PipeName(endpoint.Address);
        var name = LocalName(full);
        NamedPipeServerStream first;
        try
        {
            first = PipeListener.CreateInstance(name, true);
        }
        catch (UnauthorizedAccessException error)
        {
            throw new IpcException(IpcErrorKind.AddrInUse, $"a server already listens on `{full}`", error);
        }
        catch (IOException error)
        {
            throw IpcTransports.Wrap(error);
        }
        return new PipeListener(endpoint.Address, name, first);
    }
}
