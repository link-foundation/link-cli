using System.Net.Sockets;
using Link.Foundation.Links.Notation.Binary;

using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>Sends substitution queries to a LiNo server over any <see cref="ILinoProtocol"/>.</summary>
/// <remarks>
/// A server pushes the changes a connection subscribed to between its replies; a pushed document starts with
/// <c>(notify: N)</c>. <see cref="Request"/> sets the pushes it meets aside for <see cref="ReceiveNotification"/>, so
/// replies and pushes never get mixed up. The client is not thread safe.
/// </remarks>
public sealed class LinksClient : IDisposable
{
    private readonly TcpClient _client;
    private readonly LinoConnection _connection;
    private readonly Queue<IReadOnlyList<LinoLink>> _notifications = new();

    private LinksClient(TcpClient client, ILinoProtocol protocol)
    {
        _client = client;
        _connection = new LinoConnection(client.GetStream(), protocol);
    }

    /// <summary>The protocol used for every message.</summary>
    public ILinoProtocol Protocol => _connection.Protocol;

    /// <summary>Connects to <paramref name="host"/>:<paramref name="port"/> using <paramref name="protocol"/>.</summary>
    public static LinksClient Connect(string host, int port, ILinoProtocol protocol)
    {
        ArgumentNullException.ThrowIfNull(protocol);
        var client = new TcpClient();
        try
        {
            client.Connect(host, port);
            client.NoDelay = true;
            return new LinksClient(client, protocol);
        }
        catch (SocketException error)
        {
            client.Dispose();
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

    /// <summary>Like <see cref="Query"/>, returning the reply as canonical text.</summary>
    public string QueryText(string query) => LinoFormat.FormatDocument(Query(query));

    /// <inheritdoc/>
    public void Dispose()
    {
        _connection.Dispose();
        _client.Dispose();
    }
}
