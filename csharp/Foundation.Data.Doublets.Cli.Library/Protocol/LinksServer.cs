using System.Net;
using System.Numerics;
using System.Net.Sockets;
using Link.Foundation.Links.Notation.Binary;
using Platform.Data;
using Platform.Data.Doublets;

using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>Which protocols a server accepts.</summary>
public enum AcceptedProtocols
{
    /// <summary>Detect the protocol of every message (the default).</summary>
    Any,
    /// <summary>Only <see cref="TextLinoProtocol"/> messages.</summary>
    Text,
    /// <summary>Only <see cref="BinaryLinoProtocol"/> messages.</summary>
    Binary,
}

/// <summary>Server configuration.</summary>
public sealed record LinksServerOptions
{
    /// <summary>Print every request and reply to stderr.</summary>
    public bool Trace { get; init; }

    /// <summary>Passed to <c>AdvancedMixedQueryProcessor.Options.AutoCreateMissingReferences</c>.</summary>
    public bool AutoCreateMissingReferences { get; init; }

    /// <summary>Protocols the server answers.</summary>
    public AcceptedProtocols Accept { get; init; } = AcceptedProtocols.Any;

    /// <summary>Limits applied to messages of either protocol.</summary>
    public ProtocolLimits Limits { get; init; } = ProtocolLimits.Default;
}

/// <summary>
/// A TCP server exposing a links store through the LiNo protocols.
/// </summary>
/// <remarks>
/// Each connection gets its own thread that parses and formats messages;
/// requests are executed one at a time under a lock, so the store needs no
/// thread safety of its own. The protocol is detected per message and the
/// reply uses the same protocol, so text and binary clients can share one server.
/// </remarks>
public sealed partial class LinksServer : IDisposable
{
    private readonly TcpListener _listener;
    private readonly LinksServerOptions _options;
    private readonly object _storeLock = new();
    private readonly HashSet<TcpClient> _clients = new();
    private volatile bool _stopping;

    private LinksServer(TcpListener listener, LinksServerOptions options)
    {
        _listener = listener;
        _options = options;
    }

    /// <summary>The address the server listens on.</summary>
    public IPEndPoint LocalEndPoint => (IPEndPoint)_listener.LocalEndpoint;

    /// <summary>Binds to <paramref name="host"/>:<paramref name="port"/> (use port 0 for an ephemeral port).</summary>
    public static LinksServer Bind(string host, int port, LinksServerOptions? options = null)
    {
        ArgumentNullException.ThrowIfNull(host);
        var address = IPAddress.TryParse(host, out var parsed)
            ? parsed
            : Dns.GetHostAddresses(host).First();
        var listener = new TcpListener(address, port);
        listener.Start();
        return new LinksServer(listener, options ?? new LinksServerOptions());
    }

    /// <summary>Binds to an address written as <c>host:port</c>.</summary>
    public static LinksServer Bind(string endPoint, LinksServerOptions? options = null)
    {
        ArgumentNullException.ThrowIfNull(endPoint);
        var (host, port) = ParseEndPoint(endPoint);
        return Bind(host, port, options);
    }

    /// <summary>Splits <c>host:port</c> (or <c>[ipv6]:port</c>) into its parts.</summary>
    public static (string Host, int Port) ParseEndPoint(string endPoint)
    {
        ArgumentNullException.ThrowIfNull(endPoint);
        var separator = endPoint.LastIndexOf(':');
        if (separator <= 0 || !int.TryParse(endPoint[(separator + 1)..], out var port) || port is < 0 or > 65535)
        {
            throw new ArgumentException($"expected host:port, got '{endPoint}'", nameof(endPoint));
        }
        return (endPoint[..separator].Trim('[', ']'), port);
    }

    /// <summary>Serves <paramref name="links"/> until <see cref="Shutdown"/> is called.</summary>
    public void Serve(INamedTypesLinks<uint> links) => Serve<uint>(links);

    /// <summary>Serves the selected unsigned address type through the same protocols.</summary>
    public void Serve<T>(INamedTypesLinks<T> links)
        where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
    {
        ArgumentNullException.ThrowIfNull(links);
        var limits = _options.Limits.ForQuery(links.QueryLimits);
        var workers = new List<Thread>();
        while (!_stopping)
        {
            TcpClient client;
            try
            {
                client = _listener.AcceptTcpClient();
            }
            catch (Exception error) when (error is SocketException or ObjectDisposedException or InvalidOperationException)
            {
                break;
            }
            catch (NullReferenceException) when (_stopping)
            {
                // .NET 10 TcpListener.Stop clears its socket after disposing it.
                // AcceptTcpClient can pass its active check before that clear,
                // then dereference the cleared socket. Only shutdown permits it.
                break;
            }
            if (Register(client) is not { } stream)
            {
                continue;
            }
            var worker = new Thread(() => HandleConnection(client, stream, links, limits)) { IsBackground = true };
            workers.Add(worker);
            worker.Start();
            workers.RemoveAll(thread => !thread.IsAlive);
        }
        Shutdown();
        foreach (var worker in workers)
        {
            worker.Join();
        }
    }

    /// <summary>Stops <see cref="Serve"/> and closes every open connection.</summary>
    public void Shutdown()
    {
        lock (_clients)
        {
            _stopping = true;
            foreach (var client in _clients)
            {
                client.Dispose();
            }
            _clients.Clear();
        }
        _listener.Stop();
    }

    /// <inheritdoc/>
    public void Dispose() => Shutdown();

    /// <summary>Lists an accepted client and returns its stream, or null when its socket is already gone.</summary>
    internal NetworkStream? Register(TcpClient client)
    {
        lock (_clients)
        {
            // Shutdown disposes only listed clients, and under this lock, so this
            // one still has its socket. TcpClient is not thread safe: a Dispose in
            // the middle of GetStream makes it throw ArgumentNullException, which
            // used to kill the process when a worker got its own stream.
            // Accepted while stopping, it closes in its worker, which sees _stopping.
            TryDisableNagle(client);
            try
            {
                var stream = client.GetStream();
                _clients.Add(client);
                return stream;
            }
            catch (InvalidOperationException error)
            {
                Trace($"connection closed: {error.Message}");
                client.Dispose();
                return null;
            }
        }
    }

    /// <summary>Answers the requests on <paramref name="stream"/>, which belongs to <paramref name="client"/>.</summary>
    internal void HandleConnection(TcpClient client, NetworkStream stream, INamedTypesLinks<uint> links)
        => HandleConnection(client, stream, links, _options.Limits);

    private void HandleConnection<T>(TcpClient client, NetworkStream stream, INamedTypesLinks<T> links, ProtocolLimits limits)
        where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
    {
        try
        {
            var reader = new PacketReader(stream);
            while (!_stopping)
            {
                (IReadOnlyList<LinoLink> Document, MessageFormat Format)? message;
                try
                {
                    message = LinoProtocols.ReadAnyDocument(reader, limits);
                }
                catch (LinoProtocolException error) when (error.Kind != LinoProtocolErrorKind.Io)
                {
                    // The stream may be out of sync; answer in text and hang up.
                    MessageFormat.Text.Protocol(_options.Limits).WriteDocument(stream, ErrorDocument(error.Message));
                    Trace($"connection closed: {error.Message}");
                    return;
                }
                if (message is not { } received)
                {
                    Trace("client hung up");
                    return;
                }
                var (document, format) = received;
                var reply = Accepts(format) ? Execute(links, document) : ErrorDocument("this server does not accept this protocol");
                format.Protocol(_options.Limits).WriteDocument(stream, reply);
            }
        }
        catch (Exception error) when (error is IOException or ObjectDisposedException or SocketException or LinoProtocolException)
        {
            Trace($"connection closed: {error.Message}");
        }
        finally
        {
            lock (_clients)
            {
                _clients.Remove(client);
            }
            // Also disposes the stream.
            client.Dispose();
        }
    }

    /// <summary>Sends every reply at once; a connection that refuses still works, only slower, as in the Rust server.</summary>
    internal void TryDisableNagle(TcpClient client)
    {
        try
        {
            client.NoDelay = true;
        }
        catch (SocketException error)
        {
            Trace($"could not disable Nagle's algorithm: {error.Message}");
        }
    }

    private bool Accepts(MessageFormat format) => _options.Accept switch
    {
        AcceptedProtocols.Text => !format.IsBinary,
        AcceptedProtocols.Binary => format.IsBinary,
        _ => true,
    };

    internal IReadOnlyList<LinoLink> Execute(INamedTypesLinks<uint> links, IReadOnlyList<LinoLink> document)
        => Execute<uint>(links, document);

    internal IReadOnlyList<LinoLink> Execute<T>(INamedTypesLinks<T> links, IReadOnlyList<LinoLink> document)
        where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
    {
        lock (_storeLock)
        {
            // No request may touch the store once Serve is returning.
            if (_stopping)
            {
                return ErrorDocument("server is shutting down");
            }
            var reply = ExecuteRequestCore(links, document, _options.AutoCreateMissingReferences, _options.Trace ? Trace : null);
            if (_options.Trace) Trace($"reply: {LinoFormat.FormatDocument(reply)}");
            return reply;
        }
    }

    private void Trace(string message)
    {
        if (_options.Trace)
        {
            DiagnosticOutput.WriteErrorLine($"[server] {message}");
        }
    }

    /// <summary>The reply document for a failed request: <c>(error: 'message')</c>.</summary>
    public static IReadOnlyList<LinoLink> ErrorDocument(string message) =>
        new[] { LinoFormat.Link("error", new List<LinoLink> { LinoFormat.Reference(message) }) };

    /// <summary>Returns the message of an <c>(error: 'message')</c> reply, or null.</summary>
    public static string? ErrorMessage(IReadOnlyList<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return document is [{ Id: "error", Values: [{ Values: null, Id: var message }] }] ? message : null;
    }

    /// <summary>
    /// Executes one request against <paramref name="links"/>.
    /// </summary>
    /// <remarks>
    /// A <see cref="LinksOperation"/> is one call of the links interface, made by
    /// <see cref="RemoteLinks"/>. Any other non-empty document is a substitution
    /// query; the reply holds one <c>(before) (after)</c> line per change, exactly
    /// like <c>clink --changes</c>. The empty document asks for every link, one
    /// <c>(index: source target)</c> per line. Failures produce an <see cref="ErrorDocument"/>.
    /// </remarks>
    public static IReadOnlyList<LinoLink> ExecuteRequest(
        INamedTypesLinks<uint> links, IReadOnlyList<LinoLink> document, bool autoCreateMissingReferences = false)
        => ExecuteRequest<uint>(links, document, autoCreateMissingReferences);
}
