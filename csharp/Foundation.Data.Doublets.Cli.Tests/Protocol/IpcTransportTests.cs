using System.Collections.Concurrent;
using System.Diagnostics.CodeAnalysis;
using Foundation.Data.Doublets.Cli.Ipc;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

/// <summary>
/// Every IPC transport kind this platform supports, in process. Mirrors
/// <c>rust/src/protocol/ipc/tests.rs</c>.
/// </summary>
public sealed class IpcTransportTests
{
    /// <summary>Runs <paramref name="test"/> and fails if it takes longer than <paramref name="seconds"/>.</summary>
    internal static void Within(int seconds, Action test)
    {
        var task = Task.Run(test);
        if (!task.Wait(TimeSpan.FromSeconds(seconds)))
        {
            Assert.Fail($"the test did not finish in {seconds} s");
        }
        task.GetAwaiter().GetResult();
    }

    internal static byte[] Payload(int length, uint seed) =>
        [.. Enumerable.Range(0, length).Select(index => (byte)unchecked(((uint)index * 31 + seed) % 251))];

    /// <summary>How a test marks the end of a payload.</summary>
    internal enum Framing
    {
        /// <summary>The writer ends its outgoing direction.</summary>
        HalfClose,

        /// <summary>An 8-byte little-endian length first, for kinds that cannot half-close (Windows named pipes).</summary>
        LengthPrefix,
    }

    /// <summary>Echoes every connection until the listener is closed; returns the number of connections.</summary>
    internal static Task<int> Serve(IIpcListener listener, Framing framing = Framing.HalfClose) => Task.Factory.StartNew(() =>
    {
        var served = 0;
        while (listener.Accept() is { } stream)
        {
            served++;
            new Thread(() =>
            {
                using (stream)
                {
                    byte[] received;
                    if (framing == Framing.LengthPrefix)
                    {
                        var length = new byte[8];
                        stream.ReadExactly(length);
                        received = new byte[BitConverter.ToInt64(length)];
                        stream.ReadExactly(received);
                    }
                    else
                    {
                        using var buffer = new MemoryStream();
                        stream.CopyTo(buffer);
                        received = buffer.ToArray();
                    }
                    stream.Write(received);
                    stream.Flush();
                    if (framing == Framing.HalfClose)
                    {
                        stream.CloseWrite();
                    }
                    // Wait for the client to hang up.
                    try
                    {
                        _ = stream.Read(new byte[1]);
                    }
                    catch (IpcException)
                    {
                    }
                }
            })
            { IsBackground = true }.Start();
        }
        listener.Dispose();
        return served;
    }, TaskCreationOptions.LongRunning);

    internal static void RoundTrip(string endpoint, int length, uint seed, Framing framing = Framing.HalfClose)
    {
        using var client = IpcTransports.Connect(endpoint);
        var sent = Payload(length, seed);
        var writer = Task.Factory.StartNew(() =>
        {
            var output = client.Clone();
            if (framing == Framing.LengthPrefix)
            {
                output.Write(BitConverter.GetBytes((long)sent.Length));
            }
            output.Write(sent);
            output.Flush();
            if (framing == Framing.HalfClose)
            {
                output.CloseWrite();
            }
        }, TaskCreationOptions.LongRunning);
        byte[] echoed;
        if (framing == Framing.HalfClose)
        {
            using var buffer = new MemoryStream();
            client.CopyTo(buffer);
            echoed = buffer.ToArray();
        }
        else
        {
            echoed = new byte[length];
            client.ReadExactly(echoed);
        }
        writer.GetAwaiter().GetResult();
        Assert.Equal(sent.Length, echoed.Length);
        Assert.True(sent.AsSpan().SequenceEqual(echoed), $"{endpoint}: echoed bytes differ");
        client.Shutdown();
    }

    /// <summary>Two consecutive and two concurrent connections echo their payloads.</summary>
    internal static void CheckKind(string endpoint, int length, Framing framing = Framing.HalfClose) => Within(120, () =>
    {
        var listener = IpcTransports.Listen(endpoint);
        var address = listener.Endpoint;
        var closer = listener.Closer();
        var server = Serve(listener, framing);
        try
        {
            RoundTrip(address, length, 1, framing);
            RoundTrip(address, 1, 2, framing);
            var concurrent = Enumerable.Range(0, 2)
                .Select(index => Task.Factory.StartNew(() => RoundTrip(address, length / 2 + index, 3 + (uint)index, framing),
                    TaskCreationOptions.LongRunning))
                .ToArray();
            Task.WaitAll(concurrent);
        }
        finally
        {
            closer();
        }
        Assert.Equal(4, server.GetAwaiter().GetResult());
    });

    /// <summary>A temporary directory and the <c>KIND:DIR/links</c> endpoint in it.</summary>
    internal sealed class RendezvousDirectory : IDisposable
    {
        public RendezvousDirectory(string kind, string options = "")
        {
            Path = Directory.CreateTempSubdirectory("clink-ipc-").FullName;
            Endpoint = $"{kind}:{System.IO.Path.Combine(Path, "links")}{options}";
        }

        public string Path { get; }

        public string Endpoint { get; }

        public void Dispose()
        {
            try
            {
                Directory.Delete(Path, true);
            }
            catch (IOException)
            {
            }
        }
    }

    private static void CheckRendezvousKind(string kind, string options, int length)
    {
        using var directory = new RendezvousDirectory(kind, options);
        CheckKind(directory.Endpoint, length);
    }

    [Fact]
    public void TcpEchoes() => CheckKind("tcp:127.0.0.1:0", 300_000);

    [Fact]
    public void BareAddressIsTcp()
    {
        using var listener = IpcTransports.Listen("127.0.0.1:0");
        Assert.NotNull(listener.LocalEndPoint);
        Assert.Equal("tcp", Endpoint.Parse(listener.Endpoint).Kind);
    }

    [Fact]
    public void UdpEchoes() => CheckKind("udp:127.0.0.1:0", 100_000);

    [Fact]
    public void FileEchoes() => CheckRendezvousKind("file", "?capacity=4096", 100_000);

    [Fact]
    public void MmapEchoes() => CheckRendezvousKind("mmap", "?capacity=4096", 300_000);

    [Fact]
    public void ShmEchoes() => CheckRendezvousKind("shm", "?capacity=4096", 300_000);

    [Fact]
    public void SignalEchoes() => CheckRendezvousKind("signal", "?capacity=4096", 300_000);

    [Fact]
    public void UnixEchoes()
    {
        var directory = Directory.CreateTempSubdirectory("clink-ipc-").FullName;
        try
        {
            CheckKind($"unix:{Path.Combine(directory, "links.sock")}", 300_000);
        }
        finally
        {
            Directory.Delete(directory, true);
        }
    }

    [Fact]
    public void AbstractUnixEchoes()
    {
        if (OperatingSystem.IsLinux())
        {
            CheckKind($"unix:@clink-test-{IpcTransports.RandomId()}", 10_000);
        }
    }

    [Fact]
    public void FifoEchoes()
    {
        if (!OperatingSystem.IsWindows())
        {
            CheckRendezvousKind("fifo", "", 300_000);
        }
    }

    [Fact]
    public void PipeEchoes()
    {
        if (OperatingSystem.IsWindows())
        {
            CheckKind($"pipe:clink-test-{IpcTransports.RandomId()}", 300_000, Framing.LengthPrefix);
        }
        else
        {
            CheckRendezvousKind("pipe", "", 100_000);
        }
    }

    [Fact]
    public void ASecondPipeServerIsRefused()
    {
        if (OperatingSystem.IsWindows())
        {
            var endpoint = $"pipe:clink-test-{IpcTransports.RandomId()}";
            using var first = IpcTransports.Listen(endpoint);
            var error = Assert.Throws<IpcException>(() => IpcTransports.Listen(endpoint).Dispose());
            Assert.Equal(IpcErrorKind.AddrInUse, error.Kind);
        }
    }

    [Fact]
    public void ConnectingToAMissingPipeIsRefused()
    {
        if (OperatingSystem.IsWindows())
        {
            var error = Assert.Throws<IpcException>(() =>
                IpcTransports.Connect($"pipe:clink-test-{IpcTransports.RandomId()}").Dispose());
            Assert.Equal(IpcErrorKind.ConnectionRefused, error.Kind);
        }
    }

    [Fact]
    public void MqEchoes()
    {
        if (OperatingSystem.IsLinux())
        {
            CheckRendezvousKind("mq", "?chunk=1024", 100_000);
        }
    }

    [Fact]
    public void SysvmqEchoes()
    {
        if (OperatingSystem.IsLinux() || OperatingSystem.IsMacOS())
        {
            CheckRendezvousKind("sysvmq", "?chunk=2048&poll=1", 50_000);
        }
    }

    /// <summary>
    /// A chunk longer than a queue holds (Linux <c>msgmax</c> is 8192 bytes, a macOS queue 2048)
    /// is lowered to fit instead of failing every send.
    /// </summary>
    [Fact]
    public void SysvmqLowersAChunkTheQueuesCannotHold()
    {
        if (OperatingSystem.IsLinux() || OperatingSystem.IsMacOS())
        {
            CheckRendezvousKind("sysvmq", "?chunk=65000&poll=1", 50_000);
        }
    }

    [Fact]
    public void MailslotEchoes()
    {
        if (OperatingSystem.IsWindows())
        {
            CheckRendezvousKind("mailslot", "", 50_000);
        }
    }

    [Fact]
    public void CopydataEchoes()
    {
        if (OperatingSystem.IsWindows())
        {
            CheckRendezvousKind("copydata", "", 100_000);
        }
    }

    [Fact]
    public void AlpcEchoes()
    {
        if (OperatingSystem.IsWindows() && Environment.Is64BitProcess)
        {
            CheckRendezvousKind("alpc", "", 100_000);
        }
    }

    /// <summary>Thousands of frames each way, none of them replied to: the ports must keep up.</summary>
    [Fact]
    public void AlpcCarriesManyFrames()
    {
        if (OperatingSystem.IsWindows() && Environment.Is64BitProcess)
        {
            CheckRendezvousKind("alpc", "", 8_000_000);
        }
    }

    [Fact]
    public void EveryKindIsReported()
    {
        foreach (var kind in IpcTransports.Kinds)
        {
            var either = IpcTransports.Supports(kind, true) || IpcTransports.Supports(kind, false);
            var expected = kind switch
            {
                "fifo" => !OperatingSystem.IsWindows(),
                "mq" => OperatingSystem.IsLinux(),
                "sysvmq" => OperatingSystem.IsLinux() || OperatingSystem.IsMacOS(),
                "mailslot" or "copydata" => OperatingSystem.IsWindows(),
                "alpc" => OperatingSystem.IsWindows() && Environment.Is64BitProcess,
                _ => true,
            };
            Assert.True(either == expected, kind);
        }
        Assert.True(IpcTransports.Supports("stdio", true) && !IpcTransports.Supports("stdio", false));
        Assert.True(IpcTransports.Supports("exec", false) && !IpcTransports.Supports("exec", true));
    }

    [Fact]
    public void UnsupportedKindsFailCleanly()
    {
        foreach (var kind in IpcTransports.Kinds)
        {
            var endpoint = $"{kind}:/nonexistent/clink";
            if (!IpcTransports.Supports(kind, false) && kind != "stdio")
            {
                var error = Assert.Throws<IpcException>(() => IpcTransports.Connect(endpoint).Dispose());
                Assert.True(error.Kind == IpcErrorKind.Unsupported, $"{kind}: {error.Message}");
            }
            if (!IpcTransports.Supports(kind, true) && kind != "exec")
            {
                var error = Assert.Throws<IpcException>(() => IpcTransports.Listen(endpoint).Dispose());
                Assert.True(error.Kind == IpcErrorKind.Unsupported, $"{kind}: {error.Message}");
            }
        }
        Assert.Equal(IpcErrorKind.Unsupported,
            Assert.Throws<IpcException>(() => IpcTransports.Connect("stdio").Dispose()).Kind);
        Assert.Equal(IpcErrorKind.Unsupported,
            Assert.Throws<IpcException>(() => IpcTransports.Listen("exec:cat").Dispose()).Kind);
    }

    [Fact]
    public void ConnectingWithoutAListenerFailsQuickly() => Within(30, () =>
    {
        using var directory = new RendezvousDirectory("mmap", "?timeout=300");
        var error = Assert.Throws<IpcException>(() => IpcTransports.Connect(directory.Endpoint).Dispose());
        Assert.True(error.Kind is IpcErrorKind.NotFound or IpcErrorKind.ConnectionRefused or IpcErrorKind.TimedOut,
            error.Message);
    });

    [Fact]
    public void ClosingAListenerEndsAccept() => Within(30, () =>
    {
        using var directory = new RendezvousDirectory("file");
        using var listener = IpcTransports.Listen(directory.Endpoint);
        var closer = listener.Closer();
        var accepting = Task.Factory.StartNew(listener.Accept, TaskCreationOptions.LongRunning);
        Thread.Sleep(50);
        closer();
        Assert.Null(accepting.GetAwaiter().GetResult());
    });

    [Fact]
    public void ListenAllAcceptsOnEveryEndpoint() => Within(60, () =>
    {
        using var directory = new RendezvousDirectory("mmap");
        var listener = IpcTransports.ListenAll(["tcp:127.0.0.1:0", directory.Endpoint]);
        var tcp = listener.LocalEndPoint?.ToString() ?? throw new InvalidOperationException("a TCP address");
        Assert.Equal(2, listener.Endpoints.Count);
        var closer = listener.Closer();
        var server = Serve(listener);
        RoundTrip(tcp, 5_000, 7);
        RoundTrip(directory.Endpoint, 5_000, 8);
        closer();
        Assert.Equal(2, server.GetAwaiter().GetResult());
    });

    [Fact]
    public void RelaysStackTransports() => Within(60, () =>
    {
        // client --mmap--> relay --udp--> relay --tcp--> echo server
        var tcp = IpcTransports.Listen("tcp:127.0.0.1:0");
        var tcpEndpoint = Endpoint.Parse(tcp.Endpoint);
        var tcpCloser = tcp.Closer();
        var server = Serve(tcp);

        using var udp = IpcTransports.Listen("udp:127.0.0.1:0");
        var udpEndpoint = Endpoint.Parse(udp.Endpoint);
        var udpCloser = udp.Closer();
        var udpRelay = Task.Factory.StartNew(() => IpcTransports.Relay(udp, tcpEndpoint), TaskCreationOptions.LongRunning);

        using var directory = new RendezvousDirectory("mmap");
        using var ring = IpcTransports.Listen(directory.Endpoint);
        var ringCloser = ring.Closer();
        var ringRelay = Task.Factory.StartNew(() => IpcTransports.Relay(ring, udpEndpoint), TaskCreationOptions.LongRunning);

        RoundTrip(directory.Endpoint, 20_000, 9);

        ringCloser();
        ringRelay.GetAwaiter().GetResult();
        udpCloser();
        udpRelay.GetAwaiter().GetResult();
        tcpCloser();
        Assert.Equal(1, server.GetAwaiter().GetResult());
    });

    [Fact]
    public void ExecRunsAChildOverAnonymousPipes()
    {
        if (!OperatingSystem.IsWindows())
        {
            Within(30, () =>
            {
                RoundTrip("exec:cat", 200_000, 10);
                RoundTrip("""exec:["sh", "-c", "cat"]""", 1_000, 11);
            });
        }
    }

    [Fact]
    public void ExecCommandLines()
    {
        Assert.Equal(["clink", "--serve", "stdio"], StdioTransports.CommandLine("clink --serve stdio"));
        Assert.Equal(["C:\\Program Files\\clink.exe", "--serve", "stdio"],
            StdioTransports.CommandLine("""["C:\\Program Files\\clink.exe", "--serve", "stdio"]"""));
        Assert.Throws<IpcException>(() => StdioTransports.CommandLine("  "));
    }

    [Fact]
    public void ReadTimeoutsExpire() => Within(30, () =>
    {
        using var directory = new RendezvousDirectory("mmap");
        using var listener = IpcTransports.Listen(directory.Endpoint);
        var accepting = Task.Factory.StartNew(() => listener.Accept() ?? throw new InvalidOperationException("a stream"),
            TaskCreationOptions.LongRunning);
        using var client = IpcTransports.Connect(listener.Endpoint);
        var server = accepting.GetAwaiter().GetResult();
        client.ReadTimeoutSpan = TimeSpan.FromMilliseconds(50);
        var buffer = new byte[8];
        var error = Assert.Throws<IpcException>(() => client.Read(buffer));
        Assert.Equal(IpcErrorKind.TimedOut, error.Kind);
        server.Dispose();
        client.ReadTimeoutSpan = TimeSpan.FromSeconds(10);
        Assert.Equal(0, client.Read(buffer));
    });

    /// <summary>
    /// A socket shut down both ways still reads its end under a new timeout: macOS refuses
    /// <c>SO_RCVTIMEO</c> on it with <c>EINVAL</c>.
    /// </summary>
    [Fact]
    public void ASocketShutBothWaysStillReadsItsEnd()
    {
        if (OperatingSystem.IsWindows())
        {
            return;
        }
        var directory = Directory.CreateTempSubdirectory("clink-ipc-").FullName;
        try
        {
            Within(30, () =>
            {
                using var listener = IpcTransports.Listen($"unix:{Path.Combine(directory, "links.sock")}");
                var accepting = Task.Factory.StartNew(() => listener.Accept() ?? throw new InvalidOperationException("a stream"),
                    TaskCreationOptions.LongRunning);
                using var client = IpcTransports.Connect(listener.Endpoint);
                accepting.GetAwaiter().GetResult().Dispose();
                client.CloseWrite();
                var buffer = new byte[8];
                client.ReadTimeoutSpan = TimeSpan.FromSeconds(10);
                Assert.Equal(0, client.Read(buffer));
                client.ReadTimeoutSpan = TimeSpan.FromMilliseconds(50);
                Assert.Equal(0, client.Read(buffer));
            });
        }
        finally
        {
            Directory.Delete(directory, true);
        }
    }

    [Fact]
    public void CustomKindsCanBeRegistered()
    {
        // A custom kind that is an alias of TCP.
        IpcTransports.Register("test-alias", endpoint => IpcTransports.Connect($"tcp:{endpoint.Address}"), null);
        Assert.True(IpcTransports.Supports("test-alias", false));
        Assert.False(IpcTransports.Supports("test-alias", true));
        Assert.Throws<IpcException>(() => IpcTransports.Register("tcp", null, null));
        Assert.Throws<IpcException>(() => IpcTransports.Register("Bad Kind", null, null));
        Within(30, () =>
        {
            var listener = IpcTransports.Listen("tcp:127.0.0.1:0");
            var address = listener.Endpoint;
            var closer = listener.Closer();
            var server = Serve(listener);
            RoundTrip($"test-alias:{address}", 1_000, 12);
            closer();
            server.GetAwaiter().GetResult();
        });
    }

    [Fact]
    public void EndpointsParse()
    {
        var endpoint = Endpoint.Parse("shm:/tmp/links?capacity=1048576&timeout=500");
        Assert.Equal("shm", endpoint.Kind);
        Assert.Equal("/tmp/links", endpoint.Address);
        Assert.Equal("1048576", endpoint.Option("capacity"));
        Assert.Equal(TimeSpan.FromMilliseconds(500), endpoint.Timeout);
        Assert.Equal("tcp", Endpoint.Parse("localhost:80").Kind);
        Assert.Equal("tcp", Endpoint.Parse("[::1]:80").Kind);
        Assert.Equal("stdio", Endpoint.Parse("stdio").Kind);
        Assert.Throws<IpcException>(() => Endpoint.Parse("shm:"));
        Assert.Throws<IpcException>(() => Endpoint.Parse("shm:/tmp/x?capacity=1"));
        Assert.Throws<IpcException>(() => Endpoint.Parse("shm:/tmp/x?nonsense=1"));
        Assert.Throws<IpcException>(() => Endpoint.Parse("stdio:x"));
        const string text = "mq:/tmp/links?chunk=512";
        Assert.Equal(text, Endpoint.Parse(text).ToString());
    }

    /// <summary>Two in-memory datagram links that lose every <c>loseEvery</c>-th frame (0 loses none).</summary>
    private sealed class MemoryLink(BlockingCollection<byte[]> incoming, BlockingCollection<byte[]> outgoing, uint loseEvery)
        : IDatagrams
    {
        private uint _sent;

        public bool Send(ReadOnlySpan<byte> frame)
        {
            _sent++;
            if (loseEvery == 0 || _sent % loseEvery != 0)
            {
                outgoing.Add(frame.ToArray());
            }
            return true;
        }

        public byte[]? Receive(TimeSpan timeout) => incoming.TryTake(out var frame, timeout) ? frame : null;
    }

    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope",
        Justification = "The returned streams own the sessions; the queues live as long as the test process.")]
    private static (IpcStream A, IpcStream B) SessionPair(uint loseEvery, int chunk)
    {
        var (toA, toB) = (new BlockingCollection<byte[]>(), new BlockingCollection<byte[]>());
        var settings = new DatagramSettings(chunk, TimeSpan.FromMilliseconds(5), null, null);
        return (new IpcStream(new DatagramSession("test", new MemoryLink(toA, toB, loseEvery), settings)),
            new IpcStream(new DatagramSession("test", new MemoryLink(toB, toA, loseEvery), settings)));
    }

    [Fact]
    public void DatagramSessionsSurviveLoss() => Within(60, () =>
    {
        foreach (var loseEvery in new uint[] { 0, 3, 7 })
        {
            var (a, b) = SessionPair(loseEvery, 64);
            using (a)
            using (b)
            {
                var sent = Payload(20_000, loseEvery);
                var writer = Task.Factory.StartNew(() =>
                {
                    a.Write(sent);
                    a.CloseWrite();
                }, TaskCreationOptions.LongRunning);
                using var received = new MemoryStream();
                b.CopyTo(received);
                Assert.True(sent.AsSpan().SequenceEqual(received.ToArray()), $"lose every {loseEvery}");
                writer.GetAwaiter().GetResult();
            }
        }
    });

    /// <summary>
    /// A reader may hang up as soon as it reads the end, and the acknowledgement of that end can be lost (a full
    /// queue drops it, a removed System V queue discards it): the writer's half-close then still succeeds, as all its
    /// data was acknowledged, while a later DATA frame is a broken pipe.
    /// </summary>
    [Fact]
    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope",
        Justification = "The stream owns the session; the queues live as long as the test process.")]
    public void APeerHangingUpInsteadOfAcknowledgingTheEndStillEndsTheStream() => Within(30, () =>
    {
        var (toA, toPeer) = (new BlockingCollection<byte[]>(), new BlockingCollection<byte[]>());
        var settings = new DatagramSettings(64, TimeSpan.FromMilliseconds(5), null, null);
        using var a = new IpcStream(new DatagramSession("test", new MemoryLink(toA, toPeer, 0), settings));
        var received = new MemoryStream();
        var peer = new Thread(() =>
        {
            var accepted = 0u;
            while (toPeer.TryTake(out var frame, TimeSpan.FromSeconds(5)))
            {
                Assert.True(DatagramSession.TryParse(frame, out var kind, out var seq, out var payload));
                if (kind == DatagramSession.Data)
                {
                    // A retransmission is acknowledged again, not read twice.
                    if (seq == accepted + 1)
                    {
                        accepted = seq;
                        received.Write(payload);
                    }
                    toA.Add(DatagramSession.Frame(DatagramSession.Ack, seq, []));
                }
                else if (kind == DatagramSession.Eof)
                {
                    toA.Add(DatagramSession.Frame(DatagramSession.Abort, 0, []));
                    return;
                }
            }
        });
        peer.Start();
        a.Write("links"u8);
        a.CloseWrite();
        peer.Join();
        Assert.Equal("links"u8.ToArray(), received.ToArray());
        var error = Assert.Throws<IpcException>(() => a.Write("more"u8));
        Assert.Equal(IpcErrorKind.BrokenPipe, error.Kind);
    });

    [Fact]
    public void DatagramFramesRoundTrip()
    {
        var frame = DatagramSession.Frame(DatagramSession.Data, 7, "links"u8);
        Assert.Equal(DatagramSession.HeaderSize + 5, frame.Length);
        Assert.True(DatagramSession.TryParse(frame, out var kind, out var seq, out var payload));
        Assert.Equal(DatagramSession.Data, kind);
        Assert.Equal(7u, seq);
        Assert.Equal("links"u8.ToArray(), payload.ToArray());
        Assert.False(DatagramSession.TryParse(frame.AsSpan(0, 4), out _, out _, out _));
        Assert.Equal(1024, DatagramSession.AgreeChunk(4096, 1024));
        Assert.Equal(4096, DatagramSession.AgreeChunk(4096, null));
        Assert.Equal(16, DatagramSession.AgreeChunk(1, 1));
    }

    [Fact]
    public void RingRegionLayoutMatchesTheSpecification() =>
        Assert.Equal(64 + 2 * (128 + 4096), RingTransport.RegionSize(4096));

    [Fact]
    public void RandomIdsAreTwentyHexDigits()
    {
        var (a, b) = (IpcTransports.RandomId(), IpcTransports.RandomId());
        Assert.Equal(20, a.Length);
        Assert.True(a.All(Uri.IsHexDigit));
        Assert.NotEqual(a, b);
    }
}
