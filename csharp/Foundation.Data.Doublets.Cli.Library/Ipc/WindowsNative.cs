using System.Diagnostics.CodeAnalysis;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>The Win32 calls the Windows mechanisms share.</summary>
[SupportedOSPlatform("windows")]
internal static partial class WindowsNative
{
    public const uint ProcessQueryLimitedInformation = 0x1000;
    public const uint StillActive = 259;
    public const int ErrorAccessDenied = 5;
    public const int StdOutputHandle = -11;

    [LibraryImport("kernel32.dll", SetLastError = true)]
    private static partial nint OpenProcess(uint access, [MarshalAs(UnmanagedType.Bool)] bool inherit, uint pid);

    [LibraryImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool GetExitCodeProcess(nint process, out uint code);

    [LibraryImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static partial bool CloseHandle(nint handle);

    [LibraryImport("kernel32.dll", SetLastError = true)]
    private static partial nint GetStdHandle(int which);

    [LibraryImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool SetStdHandle(int which, nint handle);

    /// <summary>The last Win32 error as an <see cref="IpcException"/> with the system message.</summary>
    public static IpcException LastError(string? what = null) => CodeError(Marshal.GetLastPInvokeError(), what);

    public static IpcException CodeError(int code, string? what = null)
    {
        var kind = code switch
        {
            2 or 3 => IpcErrorKind.NotFound,
            ErrorAccessDenied => IpcErrorKind.PermissionDenied,
            183 or 80 => IpcErrorKind.AlreadyExists,
            109 or 232 => IpcErrorKind.BrokenPipe,
            _ => IpcErrorKind.Other,
        };
        var message = $"{new Win32Exception(code).Message} (os error {code})";
        return new IpcException(kind, what is null ? message : $"{what}: {message}");
    }

    /// <summary>Whether the process <paramref name="pid"/> is still running.</summary>
    public static bool ProcessAlive(uint pid)
    {
        if (pid == 0)
        {
            return false;
        }
        var process = OpenProcess(ProcessQueryLimitedInformation, false, pid);
        if (process == 0)
        {
            // A process of another user is still a running process.
            return Marshal.GetLastPInvokeError() == ErrorAccessDenied;
        }
        try
        {
            return !GetExitCodeProcess(process, out var code) || code == StillActive;
        }
        finally
        {
            CloseHandle(process);
        }
    }

    /// <summary>
    /// Replaces standard output with <c>NUL</c> and closes the original handle, so the reader of the original one
    /// sees its end.
    /// </summary>
    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static void CloseStdout()
    {
        var nullDevice = File.OpenHandle("NUL", FileMode.Open, FileAccess.Write);
        var raw = nullDevice.DangerousGetHandle();
        nullDevice.SetHandleAsInvalid();
        var old = GetStdHandle(StdOutputHandle);
        if (!SetStdHandle(StdOutputHandle, raw))
        {
            var error = LastError("cannot replace standard output");
            CloseHandle(raw);
            throw error;
        }
        if (old != 0 && old != -1)
        {
            CloseHandle(old);
        }
    }
}

/// <summary>
/// The Windows <c>signal</c> notifier: each side waits on its own named auto-reset event
/// <c>Local\clink-ID-s</c> or <c>Local\clink-ID-c</c> and sets the peer's.
/// </summary>
[SupportedOSPlatform("windows")]
internal static class WindowsEvents
{
    private static readonly TimeSpan Slice = TimeSpan.FromMilliseconds(100);

    private static string EventName(string id, char side) => $"Local\\clink-{id}-{side}";

    private static EventWaitHandle CreateEvent(string id, char side)
    {
        var name = EventName(id, side);
        var handle = new EventWaitHandle(false, EventResetMode.AutoReset, name, out var created);
        if (!created)
        {
            handle.Dispose();
            throw new IpcException(IpcErrorKind.AlreadyExists, $"the event `{name}` already exists");
        }
        return handle;
    }

    private static EventWaitHandle OpenEvent(string id, char side) => EventWaitHandle.OpenExisting(EventName(id, side));

    /// <summary>A thread turns this side's event into a generation every waiter sees.</summary>
    private sealed class EventNotifier : IRingNotifier
    {
        private readonly EventWaitHandle _own;
        private readonly EventWaitHandle _peer;
        private readonly GenerationWake _wake = new();
        private volatile bool _stop;

        public EventNotifier(EventWaitHandle own, EventWaitHandle peer)
        {
            _own = own;
            _peer = peer;
            new Thread(Run) { IsBackground = true, Name = "clink-event" }.Start();
        }

        private void Run()
        {
            using (_own)
            {
                while (!_stop)
                {
                    if (_own.WaitOne(Slice))
                    {
                        _wake.Bump();
                    }
                }
            }
        }

        public ulong Generation => _wake.Generation;

        public void Notify()
        {
            try
            {
                _peer.Set();
            }
            catch (ObjectDisposedException)
            {
            }
        }

        public void Wait(ulong generation, uint attempt, TimeSpan limit) => _wake.Wait(generation, attempt, limit);

        public void Dispose()
        {
            _stop = true;
            _peer.Dispose();
        }
    }

    private sealed class Setup(EventWaitHandle server, EventWaitHandle client) : IRingSignalSetup
    {
        private bool _finished;

        public IRingNotifier Finish(IpcJson ack, uint peer)
        {
            _finished = true;
            return new EventNotifier(client, server);
        }

        public void Dispose()
        {
            if (!_finished)
            {
                server.Dispose();
                client.Dispose();
            }
        }
    }

    public static (IpcJson Fields, IRingSignalSetup Setup) Client(string id)
    {
        var server = CreateEvent(id, 's');
        try
        {
            return (new IpcJson(), new Setup(server, CreateEvent(id, 'c')));
        }
        catch
        {
            server.Dispose();
            throw;
        }
    }

    public static (IpcJson Fields, IRingNotifier Notifier) Server(string id)
    {
        var own = OpenEvent(id, 's');
        try
        {
            return (new IpcJson(), new EventNotifier(own, OpenEvent(id, 'c')));
        }
        catch
        {
            own.Dispose();
            throw;
        }
    }
}
