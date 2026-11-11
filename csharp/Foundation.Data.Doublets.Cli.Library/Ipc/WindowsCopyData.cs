using System.Collections.Concurrent;
using System.Diagnostics.CodeAnalysis;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>
/// <c>WM_COPYDATA</c> (<c>copydata:DIR</c>): a hidden message-only window per side on a message-loop thread of its
/// own, each message carrying one datagram of a reliable session. A window takes frames only from the peer window it
/// learned in the handshake.
/// </summary>
/// <remarks>
/// <c>WM_COPYDATA</c> cannot tell which process sent a message: any process of the same desktop session that guesses
/// the window handle and the peer's handle can inject frames. The handshake files limit who learns the handles, and
/// the session drops frames out of sequence, but this kind is no stronger than its rendezvous directory's permissions.
/// </remarks>
[SupportedOSPlatform("windows")]
internal static unsafe partial class WindowsCopyData
{
    /// <summary><c>cds.dwData</c> of every frame: <c>LIP1</c>.</summary>
    private const nuint Magic = 0x4C49_5031;
    /// <summary>Frames waiting to be received before more are refused.</summary>
    private const int Queue = 64;
    private const uint WmDestroy = 0x0002;
    private const uint WmClose = 0x0010;
    private const uint WmCopyData = 0x004A;
    private const uint WmNcDestroy = 0x0082;
    private const nint HwndMessage = -3;
    private const uint SmtoBlock = 0x1;
    private const uint SmtoAbortIfHung = 0x2;
    private const int ErrorInvalidWindowHandle = 1400;
    private const int ErrorClassAlreadyExists = 1410;

    [StructLayout(LayoutKind.Sequential)]
    private struct WindowClass
    {
        public uint Style;
        public nint Procedure;
        public int ClassExtra;
        public int WindowExtra;
        public nint Instance;
        public nint Icon;
        public nint Cursor;
        public nint Background;
        public nint MenuName;
        public nint ClassName;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct Message
    {
        public nint Window;
        public uint Id;
        public nuint WParam;
        public nint LParam;
        public uint Time;
        public int X;
        public int Y;
        public uint Private;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct CopyDataStruct
    {
        public nuint Data;
        public uint Length;
        public nint Pointer;
    }

    [DllImport("kernel32.dll", EntryPoint = "GetModuleHandleW", SetLastError = true, ExactSpelling = true)]
    private static extern nint GetModuleHandle(nint name);

    [DllImport("user32.dll", EntryPoint = "RegisterClassW", SetLastError = true, ExactSpelling = true)]
    private static extern ushort RegisterClass(in WindowClass windowClass);

    [DllImport("user32.dll", EntryPoint = "CreateWindowExW", SetLastError = true, ExactSpelling = true)]
    private static extern nint CreateWindowEx(uint exStyle, nint className, nint windowName, uint style, int x, int y,
        int width, int height, nint parent, nint menu, nint instance, nint parameter);

    [DllImport("user32.dll", EntryPoint = "DefWindowProcW", ExactSpelling = true)]
    private static extern nint DefWindowProc(nint window, uint message, nuint wparam, nint lparam);

    [DllImport("user32.dll", EntryPoint = "GetMessageW", SetLastError = true, ExactSpelling = true)]
    private static extern int GetMessage(out Message message, nint window, uint min, uint max);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool TranslateMessage(in Message message);

    [DllImport("user32.dll", EntryPoint = "DispatchMessageW", ExactSpelling = true)]
    private static extern nint DispatchMessage(in Message message);

    [DllImport("user32.dll", EntryPoint = "PostMessageW", SetLastError = true, ExactSpelling = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool PostMessage(nint window, uint message, nuint wparam, nint lparam);

    [DllImport("user32.dll")]
    private static extern void PostQuitMessage(int code);

    [DllImport("user32.dll", EntryPoint = "SendMessageTimeoutW", SetLastError = true, ExactSpelling = true)]
    private static extern nint SendMessageTimeout(nint window, uint message, nuint wparam, nint lparam, uint flags,
        uint timeout, out nuint result);

    /// <summary>The inbox of every live window, by window handle.</summary>
    private static readonly ConcurrentDictionary<nint, Inbox> s_windows = new();
    private static readonly Lazy<nint> s_className = new(RegisterWindowClass);

    private static nint RegisterWindowClass()
    {
        // The class lives as long as the process, and so does its name.
        var name = Marshal.StringToHGlobalUni("clink-copydata");
        var windowClass = new WindowClass
        {
            Procedure = (nint)(delegate* unmanaged[Stdcall]<nint, uint, nuint, nint, nint>)&Procedure,
            Instance = GetModuleHandle(0),
            ClassName = name,
        };
        if (RegisterClass(windowClass) == 0 && Marshal.GetLastPInvokeError() is var code && code != ErrorClassAlreadyExists)
        {
            throw WindowsNative.CodeError(code, "cannot register the copydata window class");
        }
        return name;
    }

    /// <summary>Frames the window procedure took, for the session's receiver.</summary>
    private sealed class Inbox(int largest)
    {
        public readonly Queue<byte[]> Frames = new();
        public readonly int Largest = largest;
        /// <summary>The only window whose frames are taken; 0 until the handshake.</summary>
        public nint Peer;
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static nint Procedure(nint window, uint message, nuint wparam, nint lparam)
    {
        try
        {
            switch (message)
            {
                case WmCopyData:
                    return Take(window, wparam, lparam);
                case WmDestroy:
                    PostQuitMessage(0);
                    return 0;
                case WmNcDestroy:
                    s_windows.TryRemove(window, out _);
                    return DefWindowProc(window, message, wparam, lparam);
                default:
                    return DefWindowProc(window, message, wparam, lparam);
            }
        }
        catch (Exception error) when (error is not OutOfMemoryException)
        {
            // An exception must never cross the window procedure.
            IpcTransports.Trace($"copydata window procedure failed: {error.Message}");
            return 0;
        }
    }

    private static nint Take(nint window, nuint wparam, nint lparam)
    {
        if (lparam == 0 || !s_windows.TryGetValue(window, out var inbox))
        {
            return 0;
        }
        var data = (CopyDataStruct*)lparam;
        var peer = Volatile.Read(ref inbox.Peer);
        if (data->Data != Magic || peer == 0 || (nint)wparam != peer || data->Length > inbox.Largest)
        {
            return 0;
        }
        var frame = data->Length == 0 || data->Pointer == 0
            ? []
            : new ReadOnlySpan<byte>((void*)data->Pointer, (int)data->Length).ToArray();
        lock (inbox.Frames)
        {
            if (inbox.Frames.Count >= Queue)
            {
                return 0;
            }
            inbox.Frames.Enqueue(frame);
            Monitor.PulseAll(inbox.Frames);
        }
        return 1;
    }

    /// <summary>A hidden message-only window on a message-loop thread of its own.</summary>
    private sealed class Window : IDisposable
    {
        private readonly Thread _thread;
        private int _closed;

        private Window(nint handle, Inbox inbox, Thread thread)
        {
            Handle = handle;
            Inbox = inbox;
            _thread = thread;
        }

        public nint Handle { get; }

        public Inbox Inbox { get; }

        public static Window Spawn(int largest)
        {
            var className = s_className.Value;
            var inbox = new Inbox(largest);
            var created = new TaskCompletionSource<nint>(TaskCreationOptions.RunContinuationsAsynchronously);
            var thread = new Thread(() =>
            {
                var handle = CreateWindowEx(0, className, className, 0, 0, 0, 0, 0, HwndMessage, 0, GetModuleHandle(0), 0);
                if (handle == 0)
                {
                    created.SetException(WindowsNative.LastError("cannot create the copydata window"));
                    return;
                }
                // Before the loop: nothing reaches the window procedure until this thread takes messages.
                s_windows[handle] = inbox;
                created.SetResult(handle);
                while (GetMessage(out var message, 0, 0, 0) > 0)
                {
                    TranslateMessage(message);
                    DispatchMessage(message);
                }
            })
            { IsBackground = true, Name = "clink-copydata" };
            thread.Start();
            return new Window(created.Task.GetAwaiter().GetResult(), inbox, thread);
        }

        public void Dispose()
        {
            if (Interlocked.Exchange(ref _closed, 1) != 0)
            {
                return;
            }
            // Asks the window to destroy itself on its thread.
            _ = PostMessage(Handle, WmClose, 0, 0);
            if (_thread != Thread.CurrentThread)
            {
                _thread.Join(TimeSpan.FromSeconds(5));
            }
        }
    }

    private sealed class CopyDataLink(Window window, nint peerWindow, uint peer) : IDatagrams
    {
        private readonly PeerWatch _peer = new(peer);

        public bool Send(ReadOnlySpan<byte> frame)
        {
            fixed (byte* pointer = frame)
            {
                var data = new CopyDataStruct { Data = Magic, Length = (uint)frame.Length, Pointer = (nint)pointer };
                // The system copies the data into the peer's process before this call returns.
                if (SendMessageTimeout(peerWindow, WmCopyData, (nuint)window.Handle, (nint)(&data),
                        SmtoAbortIfHung | SmtoBlock, 1000, out var result) == 0)
                {
                    return Marshal.GetLastPInvokeError() == ErrorInvalidWindowHandle ? throw IpcException.HungUp() : false;
                }
                return result == 1;
            }
        }

        public byte[]? Receive(TimeSpan timeout)
        {
            var deadline = Deadline.After(timeout);
            var frames = window.Inbox.Frames;
            lock (frames)
            {
                while (true)
                {
                    if (frames.TryDequeue(out var frame))
                    {
                        return frame;
                    }
                    if (deadline.Passed)
                    {
                        return null;
                    }
                    Monitor.Wait(frames, deadline.Remaining);
                }
            }
        }

        public bool PeerAlive => _peer.Alive;

        public void Close() => window.Dispose();
    }

    private static nint PeerWindow(IpcJson fields)
    {
        var handle = (nint)(long)fields.RequireNumber("hwnd");
        return handle == 0 ? throw IpcException.InvalidData("the peer sent no window") : handle;
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IpcStream Connect(Endpoint endpoint)
    {
        var rendezvous = new Rendezvous(endpoint, "copydata");
        var ownChunk = endpoint.Number("chunk", 4096, 16, 65000);
        var window = Window.Spawn((int)ownChunk + DatagramSession.HeaderSize);
        try
        {
            var ack = rendezvous.Request(new IpcJson().With("chunk", ownChunk).With("hwnd", (ulong)window.Handle));
            var peerWindow = PeerWindow(ack);
            Volatile.Write(ref window.Inbox.Peer, peerWindow);
            var chunk = DatagramSession.AgreeChunk(ownChunk, ack.Number("chunk"));
            var peer = (uint)(ack.Number("pid") ?? rendezvous.ServerPid);
            var session = new DatagramSession("copydata", new CopyDataLink(window, peerWindow, peer),
                DatagramSettings.Local(endpoint, chunk));
            IpcTransports.Trace($"copydata connected as {rendezvous.Id}, chunk {chunk}");
            return new IpcStream(session);
        }
        catch
        {
            window.Dispose();
            throw;
        }
    }

    private sealed class CopyDataListener(Endpoint endpoint) : IIpcListener
    {
        private readonly RendezvousListener _rendezvous = new(endpoint, "copydata");

        [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
        public IpcStream? Accept() => _rendezvous.AcceptWith("copydata", request =>
        {
            var ownChunk = endpoint.Number("chunk", 4096, 16, 65000);
            var chunk = DatagramSession.AgreeChunk(ownChunk, request.Fields.Number("chunk"));
            var peerWindow = PeerWindow(request.Fields);
            var window = Window.Spawn((int)ownChunk + DatagramSession.HeaderSize);
            try
            {
                Volatile.Write(ref window.Inbox.Peer, peerWindow);
                var session = new DatagramSession("copydata", new CopyDataLink(window, peerWindow, request.Pid),
                    DatagramSettings.Local(endpoint, chunk));
                return (new IpcJson().With("chunk", (ulong)chunk).With("hwnd", (ulong)window.Handle),
                    new IpcStream(session));
            }
            catch
            {
                window.Dispose();
                throw;
            }
        });

        public string Endpoint => _rendezvous.Endpoint;

        public Action Closer() => _rendezvous.Closer();

        public void Dispose() => _rendezvous.Dispose();
    }

    public static IIpcListener Listen(Endpoint endpoint) => new CopyDataListener(endpoint);
}
