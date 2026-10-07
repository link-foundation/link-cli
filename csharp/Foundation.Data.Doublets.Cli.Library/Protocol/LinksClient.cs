using System.Net.Sockets;
using Link.Foundation.Links.Notation.Binary;

using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>Sends substitution queries to a LiNo server over any <see cref="ILinoProtocol"/>.</summary>
public sealed class LinksClient : IDisposable
{
    private readonly TcpClient _client;
    private readonly LinoConnection _connection;

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
        var reply = _connection.Request(document);
        if (LinksServer.ErrorMessage(reply) is { } message)
        {
            throw new LinoProtocolException(LinoProtocolErrorKind.Remote, message);
        }
        return reply;
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
