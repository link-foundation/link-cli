using System.Net;
using System.Numerics;
using System.Net.Sockets;
using Link.Foundation.Links.Notation.Binary;
using Platform.Data;
using Platform.Data.Doublets;

using DoubletLink = Platform.Data.Doublets.Link<uint>;
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

    /// <summary>Passed to <see cref="AdvancedMixedQueryProcessor{T}.Options.AutoCreateMissingReferences"/>.</summary>
    public bool AutoCreateMissingReferences { get; init; }

    /// <summary>Protocols the server answers.</summary>
    public AcceptedProtocols Accept { get; init; } = AcceptedProtocols.Any;

    /// <summary>Limits applied to messages of either protocol.</summary>
    public ProtocolLimits Limits { get; init; } = ProtocolLimits.Default;

    /// <summary>
    /// Who may read and write what (issue #149): every connection then sees the store as its subject
    /// may, and every request is undone whole when it fails. Null (the default) serves everything to everyone.
    /// </summary>
    /// <remarks>
    /// A subscriber is pushed only the sides of a change its subject may read, as if <see cref="Readable"/> refused the others.
    /// </remarks>
    public IPermissionPolicy<uint>? Permissions { get; init; }

    /// <summary>
    /// The subject of a token, or null when the token is not valid: answers <c>(auth: token)</c>, such as
    /// <see cref="LinksTokenTable.Authenticate"/>. Null (the default) refuses every token.
    /// </summary>
    public Func<string, uint?>? Authenticate { get; init; }

    /// <summary>The subject of a connection before any <c>(auth: token)</c>; the null link (0, the default) is a guest.</summary>
    public uint Subject { get; init; }

    /// <summary>
    /// Which links a subscriber may see; null lets every subscriber see every link.
    /// </summary>
    /// <remarks>
    /// A side of a change the subscriber may not read is left out of its push, so a link that becomes unreadable
    /// arrives as a deletion and one that becomes readable as a creation. <see cref="Permissions"/> plug in here
    /// too: with them, a subscriber sees only what its subject may read, and what this lets it see. On a server in
    /// triplet mode it judges the outer doublet of a triplet, as <c>(index: type index)</c>.
    /// </remarks>
    public Func<DoubletLink, bool>? Readable { get; init; }

    /// <summary>
    /// Pushes a connection may have waiting before the server closes it as too slow to read them.
    /// </summary>
    public int MaxPendingNotifications { get; init; } = 1024;

    /// <summary>
    /// Read every request as triplets (issue #152): <c>(create: (type from to))</c> and the rest of
    /// <see cref="TripletsOperation"/>, over a store of doublets. A server without it answers as it always did.
    /// </summary>
    public bool Triplets { get; init; }
}

/// <summary>
/// A TCP server exposing a links store through the LiNo protocols.
/// </summary>
/// <remarks>
/// Each connection gets its own thread that parses messages and another that
/// writes them; requests are executed one at a time under a lock, so the store
/// needs no thread safety of its own. The protocol is detected per message and
/// the reply uses the same protocol, so text and binary clients can share one server.
/// <para>
/// A connection may <c>(subscribe: (index source target))</c> to the links matching a
/// pattern: every later write of another connection that changes such a link is
/// pushed to it as <c>(notify: N)</c> and the changes, in the protocol of the
/// subscription request, until it sends <c>(unsubscribe: N)</c> or hangs up.
/// </para>
/// <para>
/// Every connection is a <see cref="LinksSession"/>, with a subject of its own (issue #149).
/// </para>
/// <para>
/// With <see cref="LinksServerOptions.Triplets"/> the same requests and pushes are about triplets; see
/// <see cref="TripletsOperation"/>.
/// </para>
/// </remarks>
public sealed partial class LinksServer : IDisposable
{
    private readonly TcpListener _listener;
    private readonly LinksServerOptions _options;
    private readonly object _storeLock = new();
    private readonly HashSet<TcpClient> _clients = new();
    // Read and changed under _storeLock only.
    private readonly List<(LinksConnection Connection, LinksSession? Session)> _connections = new();
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
    public void Serve(INamedTypesLinks<uint> links)
    {
        ArgumentNullException.ThrowIfNull(links);
        var sessions = new LinksSessions(links, _options);
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
            if (Register(client) is not { } stream)
            {
                continue;
            }
            var session = sessions.Open();
            var limits = _options.Limits.ForQuery(links.QueryLimits);
            var worker = new Thread(() => HandleSessionConnection(client, stream, session, limits)) { IsBackground = true };
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
    private void HandleSessionConnection(TcpClient client, NetworkStream stream, LinksSession session, ProtocolLimits limits)
    {
        var connection = new LinksConnection(client, _options.MaxPendingNotifications);
        lock (_storeLock)
        {
            _connections.Add((connection, session));
        }
        var writer = new Thread(() =>
        {
            if (connection.WriteAll((document, format) => format.Protocol(_options.Limits).WriteDocument(stream, document)) is { } error)
            {
                Trace($"connection closed: {error.Message}");
            }
        })
        { IsBackground = true };
        writer.Start();
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
                    connection.Reply(ErrorDocument(error.Message), MessageFormat.Text);
                    connection.WaitForReplies();
                    Trace($"connection closed: {error.Message}");
                    return;
                }
                if (message is not { } received)
                {
                    Trace("client hung up");
                    return;
                }
                var (document, format) = received;
                if (Accepts(format))
                {
                    Execute(session, document, connection, format);
                }
                else
                {
                    connection.Reply(ErrorDocument("this server does not accept this protocol"), format);
                }
                connection.WaitForReplies();
                if (connection.IsClosed)
                {
                    Trace("connection closed");
                    return;
                }
            }
        }
        catch (Exception error) when (error is IOException or ObjectDisposedException or SocketException or LinoProtocolException)
        {
            Trace($"connection closed: {error.Message}");
        }
        finally
        {
            // Its subscriptions go with it.
            lock (_storeLock)
            {
                _connections.RemoveAll(open => open.Connection == connection);
            }
            lock (_clients)
            {
                _clients.Remove(client);
            }
            session?.Dispose();
            // Disposes the client, and so the stream, which stops the writer.
            connection.Close();
            writer.Join();
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

    /// <summary>The number of connections the server keeps subscriptions for.</summary>
    internal int SessionCount
    {
        get
        {
            lock (_storeLock)
            {
                return _connections.Count;
            }
        }
    }

    /// <summary>
    /// Executes one request of <paramref name="session"/>, queues its reply on <paramref name="connection"/>, and
    /// pushes the changes it made to the subscriptions of every other connection; all under the store lock, so every
    /// connection gets its replies and pushes in the order the store changed.
    /// </summary>
    internal IReadOnlyList<LinoLink> Execute(
        LinksSession session,
        IReadOnlyList<LinoLink> document,
        LinksConnection? connection = null,
        MessageFormat format = default)
    {
        lock (_storeLock)
        {
            // No request may touch the store once Serve is returning.
            if (_stopping)
            {
                var stopping = ErrorDocument("server is shutting down");
                connection?.Reply(stopping, format);
                return stopping;
            }
            try
            {
                session.QueryLimits?.Document(document);
            }
            catch (InvalidOperationException error)
            {
                var rejected = ErrorDocument(error.Message);
                connection?.Reply(rejected, format);
                return rejected;
            }
            if (_options.Trace) Trace($"request: {LinoFormat.FormatDocument(document)}");
            var changes = new List<(DoubletLink Before, DoubletLink After)>();
            var tripletChanges = new List<TripletChange<uint>>();
            var reply = (connection is null ? null : ExecuteSubscription(connection, document, format))
                ?? session.Execute(document, changes, tripletChanges);
            if (_options.Trace) Trace($"reply: {LinoFormat.FormatDocument(reply)}");
            if (_options.Triplets)
            {
                reply = TripletsOperation.Pack(reply, format);
            }
            connection?.Reply(reply, format);
            if (changes.Count > 0 || tripletChanges.Count > 0)
            {
                foreach (var other in _connections.Where(other => other.Connection != connection).ToList())
                {
                    other.Connection.Notify(changes, Readable(other.Session));
                    other.Connection.NotifyTriplets(tripletChanges, Readable(other.Session));
                }
            }
            return reply;
        }
    }

    /// <summary>What a subscriber of <paramref name="session"/> may be pushed: what its subject may read, and what <see cref="LinksServerOptions.Readable"/> lets it see.</summary>
    private Func<DoubletLink, bool>? Readable(LinksSession? session)
    {
        var view = session?.View;
        var readable = _options.Readable;
        if (view is null)
        {
            return readable;
        }
        return link => view.CanRead(link) && (readable?.Invoke(link) ?? true);
    }

    /// <summary>The reply to a <c>subscribe</c> or an <c>unsubscribe</c>; null for any other request.</summary>
    private IReadOnlyList<LinoLink>? ExecuteSubscription(LinksConnection connection, IReadOnlyList<LinoLink> document, MessageFormat format)
    {
        try
        {
            if (_options.Triplets)
            {
                switch (TripletsOperation.FromDocument(document))
                {
                    case TripletsOperation.Subscribe subscribe:
                        return LinksOperation.SubscriptionDocument(connection.Subscribe(subscribe.Restriction, format));
                    case null:
                        break;
                    default:
                        return null;
                }
            }
            switch (LinksOperation.FromDocument(document))
            {
                case LinksOperation.Subscribe subscribe:
                    return LinksOperation.SubscriptionDocument(connection.Subscribe(subscribe.Restriction, format));
                case LinksOperation.Unsubscribe unsubscribe:
                    connection.Unsubscribe(unsubscribe.Subscription);
                    return Array.Empty<LinoLink>();
                default:
                    return null;
            }
        }
        catch (LinoProtocolException error)
        {
            // In triplet mode the session reports it, as any other failed request.
            return _options.Triplets ? null : ErrorDocument(error.Message);
        }
    }

    /// <summary>Answers the requests on <paramref name="stream"/>, which belongs to <paramref name="client"/>.</summary>
    internal void HandleConnection(TcpClient client, NetworkStream stream, INamedTypesLinks<uint> links)
        => HandleConnection(client, stream, links, _options.Limits.ForQuery(links.QueryLimits));

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
            Console.Error.WriteLine($"[server] {message}");
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
        INamedTypesLinks<uint> links,
        IReadOnlyList<LinoLink> document,
        bool autoCreateMissingReferences = false) =>
        ExecuteRequest(links, document, autoCreateMissingReferences, null);

    /// <summary>
    /// Executes one request and adds the net change of every link it changed to <paramref name="changed"/>, with
    /// plain numbers; a query that fails part way adds the changes it made before it failed.
    /// </summary>
    internal static IReadOnlyList<LinoLink> ExecuteRequest(
        INamedTypesLinks<uint> links,
        IReadOnlyList<LinoLink> document,
        bool autoCreateMissingReferences,
        List<(DoubletLink Before, DoubletLink After)>? changed)
    {
        return ExecuteRequestCore(links, document, autoCreateMissingReferences, null, changed);
    }

    // `(index: source target)`, naming every reference that has a name.
    private static LinoLink LinkLino(INamedTypesLinks<uint> links, DoubletLink link) =>
        LinoFormat.Link(ReferenceName(links, link.Index), new List<LinoLink>
        {
            LinoFormat.Reference(ReferenceName(links, link.Source)),
            LinoFormat.Reference(ReferenceName(links, link.Target)),
        });

    internal static string ReferenceName(INamedTypesLinks<uint> links, uint id) =>
        links.GetName(id) ?? id.ToString(System.Globalization.CultureInfo.InvariantCulture);
}
