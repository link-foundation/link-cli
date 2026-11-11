using System.Diagnostics.CodeAnalysis;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using Microsoft.Win32.SafeHandles;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>
/// Advanced Local Procedure Call (<c>alpc:DIR</c>, 64-bit Windows): each side creates the port it reads,
/// <c>\RPC Control\clink-ID-s</c> or <c>-c</c>, and connects to the peer's; each datagram message carries one frame of
/// a reliable session. The undocumented <c>NtAlpc*</c> calls are resolved from <c>ntdll.dll</c> at run time, and a
/// port accepts a connection only from the process the handshake named.
/// </summary>
[SupportedOSPlatform("windows")]
internal static unsafe class WindowsAlpc
{
    private const int StatusSuccess = 0;
    private const int StatusTimeout = 0x102;
    private const int StatusPortDisconnected = unchecked((int)0xC000_0037);
    private const uint SyncRequest = 0x0002_0000;
    private const uint ObjCaseInsensitive = 0x40;
    private const ushort LpcDatagram = 3;
    private const ushort LpcPortClosed = 5;
    private const ushort LpcClientDied = 6;
    private const ushort LpcConnectionRequest = 10;
    /// <summary><c>sizeof(PORT_MESSAGE)</c> on 64-bit Windows.</summary>
    private const int MessageHeader = 40;
    private static readonly TimeSpan Slice = TimeSpan.FromMilliseconds(100);

    [StructLayout(LayoutKind.Sequential)]
    private struct UnicodeString
    {
        public ushort Length;
        public ushort MaximumLength;
        public char* Buffer;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct ObjectAttributes
    {
        public uint Length;
        public nint RootDirectory;
        public UnicodeString* ObjectName;
        public uint Attributes;
        public nint SecurityDescriptor;
        public nint SecurityQualityOfService;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct SecurityQos
    {
        public uint Length;
        public int ImpersonationLevel;
        public byte ContextTrackingMode;
        public byte EffectiveOnly;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct PortAttributes
    {
        public uint Flags;
        public SecurityQos SecurityQos;
        public nuint MaxMessageLength;
        public nuint MemoryBandwidth;
        public nuint MaxPoolUsage;
        public nuint MaxSectionSize;
        public nuint MaxViewSize;
        public nuint MaxTotalSectionSize;
        public uint DupObjectTypes;
        public uint Reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct PortMessage
    {
        public ushort DataLength;
        public ushort TotalLength;
        public ushort Kind;
        public ushort DataInfoOffset;
        public nuint UniqueProcess;
        public nuint UniqueThread;
        public uint MessageId;
        public nuint ClientViewSize;
    }

    /// <summary>The <c>NtAlpc*</c> exports of <c>ntdll.dll</c>.</summary>
    private sealed class Ntdll
    {
        public delegate* unmanaged<nint*, ObjectAttributes*, PortAttributes*, int> Create;
        public delegate* unmanaged<nint*, UnicodeString*, ObjectAttributes*, PortAttributes*, uint, nint, PortMessage*,
            nuint*, nint, nint, long*, int> Connect;
        public delegate* unmanaged<nint*, nint, uint, ObjectAttributes*, PortAttributes*, nint, PortMessage*, nint, byte,
            int> Accept;
        public delegate* unmanaged<nint, uint, PortMessage*, nint, PortMessage*, nuint*, nint, long*, int> SendWaitReceive;
        public delegate* unmanaged<nint, uint, int> Disconnect;
    }

    private static readonly Lazy<(Ntdll? Exports, string? Error)> s_ntdll = new(Load);

    private static (Ntdll?, string?) Load()
    {
        if (!NativeLibrary.TryLoad("ntdll.dll", out var module))
        {
            return (null, "ntdll.dll is not loaded");
        }
        nint Find(string name) => NativeLibrary.TryGetExport(module, name, out var address) ? address : 0;
        var names = new[] { "NtAlpcCreatePort", "NtAlpcConnectPort", "NtAlpcAcceptConnectPort",
            "NtAlpcSendWaitReceivePort", "NtAlpcDisconnectPort" };
        var addresses = names.Select(Find).ToArray();
        if (Array.IndexOf(addresses, (nint)0) is var missing and >= 0)
        {
            return (null, $"ntdll.dll has no {names[missing]}");
        }
        return (new Ntdll
        {
            Create = (delegate* unmanaged<nint*, ObjectAttributes*, PortAttributes*, int>)addresses[0],
            Connect = (delegate* unmanaged<nint*, UnicodeString*, ObjectAttributes*, PortAttributes*, uint, nint,
                PortMessage*, nuint*, nint, nint, long*, int>)addresses[1],
            Accept = (delegate* unmanaged<nint*, nint, uint, ObjectAttributes*, PortAttributes*, nint, PortMessage*, nint,
                byte, int>)addresses[2],
            SendWaitReceive = (delegate* unmanaged<nint, uint, PortMessage*, nint, PortMessage*, nuint*, nint, long*,
                int>)addresses[3],
            Disconnect = (delegate* unmanaged<nint, uint, int>)addresses[4],
        }, null);
    }

    private static Ntdll Nt => s_ntdll.Value.Exports
        ?? throw new IpcException(IpcErrorKind.Unsupported, s_ntdll.Value.Error ?? "ntdll.dll is not loaded");

    private static IpcException StatusError(string what, int status)
    {
        var kind = unchecked((uint)status) switch
        {
            // STATUS_OBJECT_NAME_NOT_FOUND
            0xC000_0034 => IpcErrorKind.ConnectionRefused,
            0xC000_0022 => IpcErrorKind.PermissionDenied,
            0xC000_0035 => IpcErrorKind.AlreadyExists,
            _ => IpcErrorKind.Other,
        };
        return new IpcException(kind, $"{what} failed with NTSTATUS 0x{unchecked((uint)status):X8}");
    }

    private static string PortName(string id, char side) => $@"\RPC Control\clink-{id}-{side}";

    private static PortAttributes Attributes(int largest) => new()
    {
        SecurityQos = new SecurityQos
        {
            Length = (uint)sizeof(SecurityQos),
            // SecurityIdentification: the peer may identify, not impersonate.
            ImpersonationLevel = 1,
        },
        MaxMessageLength = (nuint)(largest + MessageHeader),
    };

    /// <summary>A relative timeout in 100 ns units.</summary>
    private static long Relative(TimeSpan timeout) => -Math.Max(timeout.Ticks, TimeSpan.TicksPerMillisecond);

    /// <summary>Keeps a handle open while its raw value is in use.</summary>
    private readonly struct Held : IDisposable
    {
        private readonly SafeHandle _handle;

        public Held(SafeHandle handle)
        {
            var added = false;
            handle.DangerousAddRef(ref added);
            _handle = handle;
            Value = handle.DangerousGetHandle();
        }

        public nint Value { get; }

        public void Dispose() => _handle.DangerousRelease();
    }

    /// <summary>Creates this side's inbox port.</summary>
    private static SafeFileHandle CreatePort(string id, char side, int largest)
    {
        var nt = Nt;
        var name = PortName(id, side);
        var attributes = Attributes(largest);
        nint port = 0;
        int status;
        fixed (char* chars = name)
        {
            var unicode = new UnicodeString
            {
                Length = (ushort)(name.Length * 2), MaximumLength = (ushort)(name.Length * 2), Buffer = chars,
            };
            var objectAttributes = new ObjectAttributes
            {
                Length = (uint)sizeof(ObjectAttributes), ObjectName = &unicode, Attributes = ObjCaseInsensitive,
            };
            status = nt.Create(&port, &objectAttributes, &attributes);
        }
        return status == StatusSuccess ? new SafeFileHandle(port, true) : throw StatusError("NtAlpcCreatePort", status);
    }

    /// <summary>Connects to the peer's inbox port; returns once the peer accepted.</summary>
    private static SafeFileHandle ConnectPort(string id, char side, int largest, TimeSpan timeout)
    {
        var nt = Nt;
        var name = PortName(id, side);
        var attributes = Attributes(largest);
        var message = new PortMessage { TotalLength = MessageHeader };
        var length = (nuint)MessageHeader;
        var interval = Relative(timeout);
        nint port = 0;
        int status;
        fixed (char* chars = name)
        {
            var unicode = new UnicodeString
            {
                Length = (ushort)(name.Length * 2), MaximumLength = (ushort)(name.Length * 2), Buffer = chars,
            };
            status = nt.Connect(&port, &unicode, null, &attributes, SyncRequest, 0, &message, &length, 0, 0, &interval);
        }
        return status switch
        {
            StatusSuccess => new SafeFileHandle(port, true),
            StatusTimeout => throw IpcException.TimedOut(),
            _ => throw StatusError("NtAlpcConnectPort", status),
        };
    }

    private enum Received
    {
        Nothing,
        Datagram,
        Connection,
        Closed,
    }

    /// <summary>A message buffer aligned for <c>PORT_MESSAGE</c>.</summary>
    private sealed class Buffer(int largest)
    {
        private readonly ulong[] _words = new ulong[(MessageHeader + largest + 7) / 8];

        public int Length => _words.Length * 8;

        public PortMessage Header
        {
            get
            {
                fixed (ulong* words = _words)
                {
                    return *(PortMessage*)words;
                }
            }
        }

        public byte[] Data()
        {
            var length = Math.Min(Header.DataLength, Length - MessageHeader);
            return MemoryMarshal.AsBytes(_words.AsSpan()).Slice(MessageHeader, length).ToArray();
        }

        public Received Receive(SafeHandle port, TimeSpan timeout)
        {
            var nt = Nt;
            var length = (nuint)Length;
            var interval = Relative(timeout);
            int status;
            fixed (ulong* words = _words)
            {
                var message = (PortMessage*)words;
                using var held = new Held(port);
                status = nt.SendWaitReceive(held.Value, 0, null, 0, message, &length, 0, &interval);
            }
            if (status == StatusTimeout)
            {
                return Received.Nothing;
            }
            if (status != StatusSuccess)
            {
                throw StatusError("NtAlpcSendWaitReceivePort", status);
            }
            return (Header.Kind & 0xFF) switch
            {
                LpcDatagram => Received.Datagram,
                LpcConnectionRequest => Received.Connection,
                LpcPortClosed or LpcClientDied => Received.Closed,
                _ => Received.Nothing,
            };
        }

        /// <summary>Accepts the connection request in this buffer when it comes from <paramref name="expected"/>,
        /// otherwise refuses it.</summary>
        public SafeFileHandle? Answer(SafeHandle port, uint expected, int largest)
        {
            var nt = Nt;
            var from = Header.UniqueProcess;
            var accept = from == expected;
            var attributes = Attributes(largest);
            nint accepted = 0;
            int status;
            fixed (ulong* words = _words)
            {
                var message = (PortMessage*)words;
                using var held = new Held(port);
                status = nt.Accept(&accepted, held.Value, 0, null, &attributes, 0, message, 0, accept ? (byte)1 : (byte)0);
            }
            if (!accept)
            {
                IpcTransports.Trace($"alpc refused a connection from process {from}");
                if (accepted != 0)
                {
                    new SafeFileHandle(accepted, true).Dispose();
                }
                return null;
            }
            return status == StatusSuccess
                ? new SafeFileHandle(accepted, true)
                : throw StatusError("NtAlpcAcceptConnectPort", status);
        }
    }

    /// <summary>Receives on <paramref name="inbox"/> until <paramref name="expected"/> connects.</summary>
    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    private static SafeFileHandle AcceptPeer(SafeHandle inbox, uint expected, int largest, Deadline deadline,
        CancellationToken stop)
    {
        var buffer = new Buffer(largest);
        while (true)
        {
            if (stop.IsCancellationRequested)
            {
                throw new IpcException(IpcErrorKind.Other, "the handshake ended");
            }
            if (deadline.Passed)
            {
                throw IpcException.TimedOut();
            }
            var wait = deadline.Remaining < Slice ? deadline.Remaining : Slice;
            if (buffer.Receive(inbox, wait) == Received.Connection && buffer.Answer(inbox, expected, largest) is { } accepted)
            {
                return accepted;
            }
        }
    }

    private sealed class AlpcLink(SafeFileHandle inbox, SafeFileHandle outbox, SafeFileHandle? accepted, int largest,
        uint peer) : IDatagrams
    {
        private readonly Buffer _buffer = new(largest);
        private readonly PeerWatch _peer = new(peer);
        private readonly Lock _accepting = new();
        /// <summary>This side's end of the peer's connection to the inbox.</summary>
        private SafeFileHandle? _accepted = accepted;

        public bool Send(ReadOnlySpan<byte> frame)
        {
            var nt = Nt;
            var words = new ulong[(MessageHeader + frame.Length + 7) / 8];
            var bytes = MemoryMarshal.AsBytes(words.AsSpan());
            frame.CopyTo(bytes[MessageHeader..]);
            int status;
            fixed (ulong* pointer = words)
            {
                var message = (PortMessage*)pointer;
                message->DataLength = (ushort)frame.Length;
                message->TotalLength = (ushort)(MessageHeader + frame.Length);
                using var held = new Held(outbox);
                status = nt.SendWaitReceive(held.Value, 0, message, 0, null, null, 0, null);
            }
            switch (status)
            {
                case StatusSuccess:
                    return true;
                case StatusPortDisconnected:
                    throw IpcException.HungUp();
                default:
                    IpcTransports.Trace($"alpc dropped a frame: NTSTATUS 0x{unchecked((uint)status):X8}");
                    return false;
            }
        }

        [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
        public byte[]? Receive(TimeSpan timeout)
        {
            lock (_buffer)
            {
                switch (_buffer.Receive(inbox, timeout))
                {
                    case Received.Datagram:
                        return _buffer.Data();
                    case Received.Connection:
                        lock (_accepting)
                        {
                            // Only the first connection, and only from the peer, is accepted.
                            var expected = _accepted is null ? _peer.Pid : 0;
                            if (_buffer.Answer(inbox, expected, largest) is { } handle)
                            {
                                _accepted = handle;
                            }
                        }
                        return null;
                    case Received.Closed:
                        throw IpcException.HungUp();
                    default:
                        return null;
                }
            }
        }

        public bool PeerAlive => _peer.Alive;

        public void Close()
        {
            if (s_ntdll.Value.Exports is { } nt && !outbox.IsClosed)
            {
                using var held = new Held(outbox);
                _ = nt.Disconnect(held.Value, 0);
            }
            outbox.Dispose();
            lock (_accepting)
            {
                _accepted?.Dispose();
                _accepted = null;
            }
            inbox.Dispose();
        }
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static IpcStream Connect(Endpoint endpoint)
    {
        _ = Nt;
        var rendezvous = new Rendezvous(endpoint, "alpc");
        var ownChunk = endpoint.Number("chunk", 4096, 16, 65000);
        var largest = (int)ownChunk + DatagramSession.HeaderSize;
        var inbox = CreatePort(rendezvous.Id, 'c', largest);
        var deadline = Deadline.After(endpoint.Timeout);
        var serverPid = rendezvous.ServerPid;
        SafeFileHandle? accepted = null;
        SafeFileHandle? outbox = null;
        try
        {
            using var stop = new CancellationTokenSource();
            var acceptor = Task.Factory.StartNew(() => AcceptPeer(inbox, serverPid, largest, deadline, stop.Token),
                CancellationToken.None, TaskCreationOptions.LongRunning, TaskScheduler.Default);
            IpcJson ack;
            try
            {
                ack = rendezvous.Request(new IpcJson().With("chunk", ownChunk));
            }
            catch
            {
                stop.Cancel();
                try
                {
                    acceptor.GetAwaiter().GetResult().Dispose();
                }
                catch (IpcException)
                {
                    // The handshake error below is the one to report.
                }
                throw;
            }
            accepted = acceptor.GetAwaiter().GetResult();
            var chunk = DatagramSession.AgreeChunk(ownChunk, ack.Number("chunk"));
            var peer = (uint)(ack.Number("pid") ?? serverPid);
            outbox = ConnectPort(rendezvous.Id, 's', largest, deadline.Remaining);
            var session = new DatagramSession("alpc", new AlpcLink(inbox, outbox, accepted, largest, peer),
                DatagramSettings.Local(endpoint, chunk));
            IpcTransports.Trace($"alpc connected as {rendezvous.Id}, chunk {chunk}");
            return new IpcStream(session);
        }
        catch
        {
            outbox?.Dispose();
            accepted?.Dispose();
            inbox.Dispose();
            throw;
        }
    }

    private sealed class AlpcListener(Endpoint endpoint) : IIpcListener
    {
        private readonly RendezvousListener _rendezvous = new(endpoint, "alpc");

        [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
        public IpcStream? Accept() => _rendezvous.AcceptWith("alpc", request =>
        {
            var ownChunk = endpoint.Number("chunk", 4096, 16, 65000);
            var requested = request.Fields.Number("chunk");
            var chunk = DatagramSession.AgreeChunk(ownChunk, requested);
            var largest = (int)Math.Max(ownChunk, requested ?? ownChunk) + DatagramSession.HeaderSize;
            var inbox = CreatePort(request.Id, 's', largest);
            SafeFileHandle outbox;
            try
            {
                outbox = ConnectPort(request.Id, 'c', largest, endpoint.Timeout);
            }
            catch
            {
                inbox.Dispose();
                throw;
            }
            // The session's receiver accepts the client's connection.
            var session = new DatagramSession("alpc", new AlpcLink(inbox, outbox, null, largest, request.Pid),
                DatagramSettings.Local(endpoint, chunk));
            return (new IpcJson().With("chunk", (ulong)chunk), new IpcStream(session));
        });

        public string Endpoint => _rendezvous.Endpoint;

        public Action Closer() => _rendezvous.Closer();

        public void Dispose() => _rendezvous.Dispose();
    }

    public static IIpcListener Listen(Endpoint endpoint)
    {
        _ = Nt;
        return new AlpcListener(endpoint);
    }
}
