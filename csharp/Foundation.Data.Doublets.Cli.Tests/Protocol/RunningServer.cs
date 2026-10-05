using Foundation.Data.Doublets.Cli.Protocol;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

/// <summary>A <see cref="LinksServer"/> serving a fresh temporary store on a background thread.</summary>
internal sealed class RunningServer : IDisposable
{
    private readonly string _databaseFilename = Path.GetTempFileName();
    private readonly LinksServer _server;
    private readonly Thread _thread;
    private Exception? _failure;

    public RunningServer(LinksServerOptions? options = null)
    {
        _server = LinksServer.Bind("127.0.0.1", 0, options);
        _thread = new Thread(() =>
        {
            try
            {
                using var links = new NamedTypesDecorator<uint>(_databaseFilename);
                _server.Serve(links);
            }
            catch (Exception error)
            {
                _failure = error;
            }
        })
        { IsBackground = true };
        _thread.Start();
    }

    public int Port => _server.LocalEndPoint.Port;

    public LinksClient Client(ILinoProtocol protocol) => LinksClient.Connect("127.0.0.1", Port, protocol);

    public RemoteLinks Remote(ILinoProtocol protocol) => new(Client(protocol));

    public void Stop()
    {
        _server.Shutdown();
        Assert.True(_thread.Join(TimeSpan.FromSeconds(30)), "the server did not stop");
        Assert.Null(_failure);
    }

    public void Dispose()
    {
        Stop();
        File.Delete(_databaseFilename);
        File.Delete(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(_databaseFilename));
    }

    /// <summary>The text protocol and the binary protocol with every option.</summary>
    public static IEnumerable<ILinoProtocol> Protocols() =>
        LinoProtocolCodecTests.AllOptions()
            .Select(options => (ILinoProtocol)new BinaryLinoProtocol(options))
            .Prepend(new TextLinoProtocol());
}
