using System.Diagnostics.CodeAnalysis;
using System.Net.Sockets;
using Foundation.Data.Doublets.Cli.Ipc;
using Link.Foundation.Links.Notation.Binary;

using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>Sends substitution queries to a LiNo server over any <see cref="ILinoProtocol"/> and any IPC transport.</summary>
/// <remarks>
/// A server pushes the changes a connection subscribed to between its replies; a pushed document starts with
/// <c>(notify: N)</c>. <see cref="Request"/> sets the pushes it meets aside for <see cref="ReceiveNotification"/>, so
/// replies and pushes never get mixed up. The client is not thread safe.
/// </remarks>
public sealed class LinksClient : IDisposable
{
    private readonly LinoConnection _connection;
    private readonly Queue<IReadOnlyList<LinoLink>> _notifications = new();

    private LinksClient(IpcStream stream, ILinoProtocol protocol)
    {
        Kind = stream.Kind;
        _connection = new LinoConnection(stream, protocol);
    }

    /// <summary>The protocol used for every message.</summary>
    public ILinoProtocol Protocol => _connection.Protocol;

    /// <summary>The transport kind the client is connected through, such as <c>tcp</c> or <c>unix</c>.</summary>
    public string Kind { get; }

    /// <summary>Connects to <paramref name="host"/>:<paramref name="port"/> over TCP using <paramref name="protocol"/>.</summary>
    [SuppressMessage("Reliability", "CA2000:Dispose objects before losing scope", Justification = IpcTransports.OwnershipTransferred)]
    public static LinksClient Connect(string host, int port, ILinoProtocol protocol)
    {
        ArgumentNullException.ThrowIfNull(protocol);
        var socket = new Socket(SocketType.Stream, ProtocolType.Tcp);
        try
        {
            socket.Connect(host, port);
            return new LinksClient(IpcTransports.FromTcpSocket(socket), protocol);
        }
        catch (SocketException error)
        {
            socket.Dispose();
            throw new LinoProtocolException(LinoProtocolErrorKind.Io, error.Message, error);
        }
    }

    /// <summary>Connects to an address written as <c>host:port</c>.</summary>
    public static LinksClient Connect(string endPoint, ILinoProtocol protocol)
    {
        var (host, port) = LinksServer.ParseEndPoint(endPoint);
        return Connect(host, port, protocol);
    }

    /// <summary>
    /// Connects to an IPC endpoint (see <see cref="Ipc.Endpoint"/>), such as <c>127.0.0.1:8080</c>,
    /// <c>unix:/tmp/links.sock</c>, <c>pipe:links</c> or <c>shm:/tmp/links</c>, using <paramref name="protocol"/>.
    /// </summary>
    /// <exception cref="LinoProtocolException">The endpoint is not valid or cannot be reached, as an <see cref="LinoProtocolErrorKind.Io"/> error.</exception>
    public static LinksClient ConnectEndpoint(string endpoint, ILinoProtocol protocol)
    {
        ArgumentNullException.ThrowIfNull(endpoint);
        ArgumentNullException.ThrowIfNull(protocol);
        return FromStream(LinksServer.Transport(() => IpcTransports.Connect(endpoint)), protocol);
    }

    /// <summary>Talks to a server through an open <see cref="IpcStream"/>, which the client then owns.</summary>
    public static LinksClient FromStream(IpcStream stream, ILinoProtocol protocol)
    {
        ArgumentNullException.ThrowIfNull(stream);
        ArgumentNullException.ThrowIfNull(protocol);
        return new LinksClient(stream, protocol);
    }

    /// <summary>
    /// Sends a parsed document and returns the reply document. An
    /// <c>(error: 'message')</c> reply raises a <see cref="LinoProtocolErrorKind.Remote"/> error.
    /// </summary>
    public IReadOnlyList<LinoLink> Request(IReadOnlyList<LinoLink> document)
    {
        _connection.Send(document);
        while (true)
        {
            var reply = _connection.Receive() ?? throw LinoProtocolException.Malformed("connection closed before the reply");
            if (LinksOperation.IsNotification(reply))
            {
                _notifications.Enqueue(reply);
                continue;
            }
            if (LinksServer.ErrorMessage(reply) is { } message)
            {
                throw new LinoProtocolException(LinoProtocolErrorKind.Remote, message);
            }
            return reply;
        }
    }

    /// <summary>The number of pushes received and not yet taken by <see cref="ReceiveNotification"/>.</summary>
    public int PendingNotifications => _notifications.Count;

    /// <summary>
    /// The next document the server pushed, <c>(notify: N)</c> and its changes, waiting up to
    /// <paramref name="timeout"/> for one; null when none arrived in time.
    /// </summary>
    /// <exception cref="LinoProtocolException">
    /// The connection closed, or the server sent something other than a push while no request was waiting.
    /// </exception>
    public IReadOnlyList<LinoLink>? ReceiveNotification(TimeSpan timeout)
    {
        if (_notifications.TryDequeue(out var queued))
        {
            return queued;
        }
        if (!_connection.WaitForMessage(timeout))
        {
            return null;
        }
        var document = _connection.Receive()
            ?? throw new LinoProtocolException(LinoProtocolErrorKind.Io, "connection closed");
        return LinksOperation.IsNotification(document)
            ? document
            : throw LinoProtocolException.Malformed($"the server sent a reply nobody asked for: {LinoFormat.FormatDocument(document)}");
    }

    /// <summary>Runs a LiNo substitution query; the empty query reads every link.</summary>
    public IReadOnlyList<LinoLink> Query(string query) =>
        Request(LinoProtocolException.Wrap(() => LinoFormat.ParseDocument(query)));

    /// <summary>Sends original query text to a peer supporting the explicit query envelope.
    /// Never retries a mutation through a different representation.</summary>
    public IReadOnlyList<LinoLink> QueryLossless(string query) => Request(LinksServer.QueryDocument(query));

    /// <summary>Like <see cref="Query"/>, returning the reply as canonical text.</summary>
    public string QueryText(string query) => LinoFormat.FormatDocument(Query(query));

    /// <summary>
    /// Sends <c>(auth: token)</c>: every later request of the connection runs as the subject the server gives
    /// the token, which this returns (issue #149). A token the server does not know raises a
    /// <see cref="LinoProtocolErrorKind.Remote"/> error.
    /// </summary>
    public uint Authenticate(string token)
    {
        ArgumentNullException.ThrowIfNull(token);
        var reply = Request(new[] { LinoFormat.Link("auth", new List<LinoLink> { LinoFormat.Reference(token) }) });
        return reply is [{ Id: "subject", Values: [{ Values: null, Id: { } subject }] }]
            && uint.TryParse(subject, System.Globalization.NumberStyles.None, System.Globalization.CultureInfo.InvariantCulture, out var index)
            ? index
            : throw LinoProtocolException.Malformed($"expected a subject reply, found {LinoFormat.FormatDocument(reply)}");
    }

    /// <inheritdoc/>
    public void Dispose() => _connection.Dispose();
}
