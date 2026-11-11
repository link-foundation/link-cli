using System.Diagnostics.CodeAnalysis;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using Microsoft.Win32.SafeHandles;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>
/// Mailslots (<c>mailslot:DIR</c>): one write-only slot per direction, so the server and the client each create the
/// slot they read, <c>\\.\mailslot\clink-ID-s</c> or <c>-c</c>, and open the one they write. Each message carries one
/// datagram of a reliable session.
/// </summary>
[SupportedOSPlatform("windows")]
internal static unsafe partial class WindowsMailslot
{
    private const uint GenericWrite = 0x40000000;
    private const uint FileShareReadWrite = 1 | 2;
    private const uint OpenExisting = 3;
    private const uint FileAttributeNormal = 0x80;
    private const int ErrorSemTimeout = 121;
    private const uint Slice = 100;

    [LibraryImport("kernel32.dll", EntryPoint = "CreateMailslotW", SetLastError = true, StringMarshalling = StringMarshalling.Utf16)]
    private static partial SafeFileHandle CreateMailslot(string name, uint maxMessageSize, uint readTimeout, nint security);

    [LibraryImport("kernel32.dll", EntryPoint = "CreateFileW", SetLastError = true, StringMarshalling = StringMarshalling.Utf16)]
    private static partial SafeFileHandle CreateFile(string name, uint access, uint share, nint security, uint disposition,
        uint flags, nint template);

    [LibraryImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool SetMailslotInfo(SafeFileHandle mailslot, uint readTimeout);

    [LibraryImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool WriteFile(SafeFileHandle file, byte* buffer, uint length, out uint written, nint overlapped);

    [LibraryImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool ReadFile(SafeFileHandle file, byte* buffer, uint length, out uint read, nint overlapped);

    private static string Name(string id, char side) => $@"\\.\mailslot\clink-{id}-{side}";

    private static SafeFileHandle Create(string id, char side, int size)
    {
        var handle = CreateMailslot(Name(id, side), (uint)size, Slice, 0);
        if (handle.IsInvalid)
        {
            var error = WindowsNative.LastError($"cannot create the mailslot `{Name(id, side)}`");
            handle.Dispose();
            throw error;
        }
        return handle;
    }

    private static SafeFileHandle Open(string id, char side)
    {
        var handle = CreateFile(Name(id, side), GenericWrite, FileShareReadWrite, 0, OpenExisting, FileAttributeNormal, 0);
        if (handle.IsInvalid)
        {
            var error = WindowsNative.LastError($"cannot open the mailslot `{Name(id, side)}`");
            handle.Dispose();
            throw error;
        }
        return handle;
    }

    /// <summary>This side's mailslot and a write handle of the peer's.</summary>
    private sealed class MailslotLink(SafeFileHandle inbox, SafeFileHandle outbox, int size, uint peer) : IDatagrams
    {
        private readonly byte[] _buffer = new byte[size];
        private readonly PeerWatch _peer = new(peer);
        private uint _timeout = Slice;

        public bool Send(ReadOnlySpan<byte> frame)
        {
            fixed (byte* pointer = frame)
            {
                if (!WriteFile(outbox, pointer, (uint)frame.Length, out _, 0))
                {
                    IpcTransports.Trace($"mailslot dropped a frame: {WindowsNative.LastError().Message}");
                    return false;
                }
            }
            return true;
        }

        public byte[]? Receive(TimeSpan timeout)
        {
            var wait = (uint)Math.Clamp(Math.Ceiling(timeout.TotalMilliseconds), 0, uint.MaxValue - 1);
            if (wait != _timeout)
            {
                if (!SetMailslotInfo(inbox, wait))
                {
                    throw WindowsNative.LastError("cannot set the mailslot timeout");
                }
                _timeout = wait;
            }
            uint read;
            fixed (byte* pointer = _buffer)
            {
                if (!ReadFile(inbox, pointer, (uint)_buffer.Length, out read, 0))
                {
                    var code = Marshal.GetLastPInvokeError();
                    if (code == ErrorSemTimeout)
                    {
                        return null;
                    }
                    throw WindowsNative.CodeError(code, "cannot read the mailslot");
                }
            }
            return _buffer.AsSpan(0, (int)read).ToArray();
        }

        public bool PeerAlive => _peer.Alive;

        public void Close()
        {
            inbox.Dispose();
            outbox.Dispose();
        }
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IpcStream Connect(Endpoint endpoint)
    {
        var rendezvous = new Rendezvous(endpoint, "mailslot");
        var ownChunk = endpoint.Number("chunk", 4096, 16, 65000);
        var size = (int)ownChunk + DatagramSession.HeaderSize;
        var inbox = Create(rendezvous.Id, 'c', size);
        SafeFileHandle? outbox = null;
        try
        {
            var ack = rendezvous.Request(new IpcJson().With("chunk", ownChunk));
            var chunk = DatagramSession.AgreeChunk(ownChunk, ack.Number("chunk"));
            var peer = (uint)(ack.Number("pid") ?? rendezvous.ServerPid);
            outbox = Open(rendezvous.Id, 's');
            var session = new DatagramSession("mailslot", new MailslotLink(inbox, outbox, size, peer),
                DatagramSettings.Local(endpoint, chunk));
            IpcTransports.Trace($"mailslot connected as {rendezvous.Id}, chunk {chunk}");
            return new IpcStream(session);
        }
        catch
        {
            inbox.Dispose();
            outbox?.Dispose();
            throw;
        }
    }

    private sealed class MailslotListener(Endpoint endpoint) : IIpcListener
    {
        private readonly RendezvousListener _rendezvous = new(endpoint, "mailslot");

        [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
        public IpcStream? Accept() => _rendezvous.AcceptWith("mailslot", request =>
        {
            var ownChunk = endpoint.Number("chunk", 4096, 16, 65000);
            var chunk = DatagramSession.AgreeChunk(ownChunk, request.Fields.Number("chunk"));
            var size = (int)ownChunk + DatagramSession.HeaderSize;
            var outbox = Open(request.Id, 'c');
            SafeFileHandle inbox;
            try
            {
                inbox = Create(request.Id, 's', size);
            }
            catch
            {
                outbox.Dispose();
                throw;
            }
            var session = new DatagramSession("mailslot", new MailslotLink(inbox, outbox, size, request.Pid),
                DatagramSettings.Local(endpoint, chunk));
            return (new IpcJson().With("chunk", (ulong)chunk), new IpcStream(session));
        });

        public string Endpoint => _rendezvous.Endpoint;

        public Action Closer() => _rendezvous.Closer();

        public void Dispose() => _rendezvous.Dispose();
    }

    public static IIpcListener Listen(Endpoint endpoint) => new MailslotListener(endpoint);
}
