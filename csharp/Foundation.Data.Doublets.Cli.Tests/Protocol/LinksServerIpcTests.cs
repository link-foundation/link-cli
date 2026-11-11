using Foundation.Data.Doublets.Cli.Ipc;
using Foundation.Data.Doublets.Cli.Protocol;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

/// <summary>
/// The links server and client over every IPC transport kind this platform supports (issue #140): the LiNo text
/// and binary protocols stack on any of them. Mirrors the server tests of <c>rust/src/protocol/ipc/tests.rs</c>.
/// </summary>
public sealed class LinksServerIpcTests
{
    private static readonly TimeSpan PushTimeout = TimeSpan.FromSeconds(10);

    /// <summary>The kinds a server can listen on here, with the options the transport tests use.</summary>
    public static TheoryData<string, string> Kinds()
    {
        var kinds = new TheoryData<string, string>
        {
            { "udp", "" },
            { "file", "?capacity=4096" },
            { "mmap", "?capacity=4096" },
            { "shm", "?capacity=4096" },
            { "signal", "?capacity=4096" },
            { "unix", "" },
        };
        if (!OperatingSystem.IsWindows())
        {
            kinds.Add("fifo", "");
            kinds.Add("pipe", "");
        }
        else
        {
            kinds.Add("pipe", "");
            kinds.Add("mailslot", "");
            kinds.Add("copydata", "");
        }
        if (IpcTransports.Supports("alpc", true))
        {
            kinds.Add("alpc", "");
        }
        if (IpcTransports.Supports("mq", true))
        {
            kinds.Add("mq", "?chunk=1024");
        }
        if (IpcTransports.Supports("sysvmq", true))
        {
            kinds.Add("sysvmq", "?chunk=2048&poll=1");
        }
        return kinds;
    }

    /// <summary>A fresh endpoint of <paramref name="kind"/> in <paramref name="directory"/>.</summary>
    private static string EndpointOf(string kind, string options, string directory) => kind switch
    {
        "udp" => "udp:127.0.0.1:0",
        "unix" => $"unix:{Path.Combine(directory, "links.sock")}",
        "pipe" when OperatingSystem.IsWindows() => $"pipe:clink-test-{IpcTransports.RandomId()}",
        _ => $"{kind}:{Path.Combine(directory, "links")}{options}",
    };

    private static void InDirectory(Action<string> test)
    {
        var directory = Directory.CreateTempSubdirectory("clink-server-ipc-").FullName;
        try
        {
            IpcTransportTests.Within(120, () => test(directory));
        }
        finally
        {
            try
            {
                Directory.Delete(directory, true);
            }
            catch (IOException)
            {
            }
        }
    }

    [Theory]
    [MemberData(nameof(Kinds))]
    public void EveryKindCarriesBothProtocolsAndPushes(string kind, string options) => InDirectory(directory =>
    {
        using var server = new RunningServer(null, EndpointOf(kind, options, directory));
        Assert.Throws<InvalidOperationException>(() => server.Server.LocalEndPoint);
        foreach (var protocol in new ILinoProtocol[] { new TextLinoProtocol(), new BinaryLinoProtocol() })
        {
            using var subscriber = server.Remote(protocol);
            using var writer = server.Client(protocol);
            Assert.Equal(kind, writer.Kind);
            var subscription = subscriber.Subscribe(LinksRestriction.All);
            var created = writer.QueryText("() ((1 1))");
            Assert.Contains("1 1", created, StringComparison.Ordinal);
            // Waiting for a push times out through the transport, not the socket, on every kind.
            Assert.NotNull(subscriber.ReceiveNotification(PushTimeout));
            subscriber.Unsubscribe(subscription);
            writer.QueryText("((1: 1 1)) ()");
        }
    });

    [Fact]
    public void ServersListenOnSeveralKindsAndShareTheStore() => InDirectory(directory =>
    {
        var unix = $"unix:{Path.Combine(directory, "links.sock")}";
        using var server = new RunningServer(null, "127.0.0.1:0", unix);
        Assert.Equal(2, server.Server.Endpoints.Count);
        Assert.Equal(string.Join(' ', server.Server.Endpoints), server.Server.Endpoint);
        Assert.NotEqual(0, server.Server.LocalEndPoint.Port);
        using var overUnix = server.Client(1, new TextLinoProtocol());
        using var overTcp = LinksClient.Connect("127.0.0.1", server.Port, new BinaryLinoProtocol());
        Assert.Equal("unix", overUnix.Kind);
        Assert.Equal("tcp", overTcp.Kind);
        overUnix.Query("() ((1 1))");
        Assert.Equal("(1: 1 1)", overTcp.QueryText(""));
    });

    [Fact]
    public void RemoteLinksConnectToAnyEndpoint() => InDirectory(directory =>
    {
        var endpoint = $"shm:{Path.Combine(directory, "links")}?capacity=4096";
        using var server = new RunningServer(null, endpoint);
        using (var remote = RemoteLinks.ConnectEndpoint(server.Server.Endpoint, new BinaryLinoProtocol()))
        {
            remote.Create(null, null);
        }
        using var generic = RemoteLinks<ulong>.ConnectEndpoint(server.Server.Endpoint, new TextLinoProtocol());
        Assert.Equal(1ul, generic.Count(null));
    });

    [Fact]
    public void BadEndpointsAreIoErrors()
    {
        var connect = Assert.Throws<LinoProtocolException>(() => LinksClient.ConnectEndpoint("bogus:x", new TextLinoProtocol()));
        Assert.Equal(LinoProtocolErrorKind.Io, connect.Kind);
        var listen = Assert.Throws<LinoProtocolException>(() => LinksServer.Listen("bogus:x"));
        Assert.Equal(LinoProtocolErrorKind.Io, listen.Kind);
        var none = Assert.Throws<LinoProtocolException>(() => LinksServer.ListenAll([]));
        Assert.Equal(LinoProtocolErrorKind.Io, none.Kind);
        var missing = Assert.Throws<LinoProtocolException>(() =>
            LinksClient.ConnectEndpoint($"unix:{Path.Combine(Path.GetTempPath(), $"clink-missing-{IpcTransports.RandomId()}.sock")}", new TextLinoProtocol()));
        Assert.Equal(LinoProtocolErrorKind.Io, missing.Kind);
    }

    [Fact]
    [System.Diagnostics.CodeAnalysis.SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = "The server owns the listener.")]
    public void AServerEndsWhenItsListenerAcceptsNoMore()
    {
        // A listener whose only connection ended, as stdio's, ends Serve with no Shutdown.
        using var log = new StringWriter();
        var standardError = Console.Error;
        Console.SetError(log);
        try
        {
            using var tcp = new System.Net.Sockets.TcpListener(System.Net.IPAddress.Loopback, 0);
            tcp.Start();
            var near = IpcTransports.Connect($"127.0.0.1:{((System.Net.IPEndPoint)tcp.LocalEndpoint).Port}");
            var once = new OneConnection(IpcTransports.FromTcpSocket(tcp.AcceptSocket()));
            using var server = LinksServer.FromListener(once, new LinksServerOptions { Trace = true });
            var databaseFilename = Path.GetTempFileName();
            try
            {
                var serving = new Thread(() =>
                {
                    using var links = new NamedTypesDecorator<uint>(databaseFilename);
                    server.Serve(links);
                })
                { IsBackground = true };
                serving.Start();
                using (var client = LinksClient.FromStream(near, new TextLinoProtocol()))
                {
                    Assert.Equal("tcp", client.Kind);
                    client.Query("() ((1 1))");
                }
                once.End();
                Assert.True(serving.Join(TimeSpan.FromSeconds(30)), "the server did not end");
            }
            finally
            {
                File.Delete(databaseFilename);
                File.Delete(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(databaseFilename));
            }
        }
        finally
        {
            Console.SetError(standardError);
        }
        var lines = log.ToString();
        Assert.Contains("[server] tcp connection accepted", lines, StringComparison.Ordinal);
        Assert.Contains("[server] once accepts no more connections", lines, StringComparison.Ordinal);
    }

    /// <summary>A listener of exactly one connection, then of none, like <c>stdio</c>.</summary>
    private sealed class OneConnection(IpcStream stream) : IIpcListener
    {
        private IpcStream? _stream = stream;
        private readonly ManualResetEventSlim _ended = new();

        public IpcStream? Accept()
        {
            if (Interlocked.Exchange(ref _stream, null) is { } first)
            {
                return first;
            }
            // Like stdio, accepts nothing more once its only connection ended.
            _ended.Wait();
            return null;
        }

        public void End() => _ended.Set();

        public string Endpoint => "once";

        public Action Closer() => () => { };

        public void Dispose() => _ended.Dispose();
    }
}
