// What a client does with replies a real server does not send, and the members only other hosts use.
using System.Net;
using System.Net.Sockets;
using Foundation.Data.Doublets.Cli.Protocol;
using Link.Foundation.Links.Notation;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

public sealed class ScriptedPeerTests
{
    /// <summary>Answers each request with the next documents of a script, then hangs up.</summary>
    private sealed class ScriptedPeer : IDisposable
    {
        private readonly TcpListener _listener = new(IPAddress.Loopback, 0);
        private readonly Thread _thread;

        public ScriptedPeer(params IReadOnlyList<LinoLink>[][] answers)
        {
            _listener.Start();
            _thread = new Thread(() =>
            {
                // An escape from this thread would end the whole test host. A client that
                // left, or Dispose stopping the listener (.NET 10 can raise a null reference
                // there), only ends the script.
                try
                {
                    using var client = _listener.AcceptTcpClient();
                    using var connection = new LinoConnection(client.GetStream(), new TextLinoProtocol());
                    foreach (var answer in answers)
                    {
                        if (connection.Receive() is null) return;
                        foreach (var document in answer) connection.Send(document);
                    }
                }
                catch (Exception error) when (error is SocketException or IOException or ObjectDisposedException
                    or InvalidOperationException or NullReferenceException)
                {
                }
            })
            { IsBackground = true };
            _thread.Start();
        }

        public int Port => ((IPEndPoint)_listener.LocalEndpoint).Port;

        public string EndPoint => $"127.0.0.1:{Port}";

        public void Dispose()
        {
            _listener.Stop();
            _thread.Join(TimeSpan.FromSeconds(5));
        }
    }

    private static LinoLink Group(params LinoLink[] values) => new(null, values.ToList());

    private static IReadOnlyList<LinoLink> Notification(uint subscription) =>
        LinksOperation.NotificationDocument(subscription, [(new DoubletLink(1, 1, 1), new DoubletLink(1, 1, 2))]);

    [Fact]
    public void ANotificationBeforeTheReplyWaitsItsTurn()
    {
        IReadOnlyList<LinoLink> reply = [new LinoLink("count", [new LinoLink("0")])];
        using var peer = new ScriptedPeer([Notification(7), reply]);
        using var client = LinksClient.Connect("127.0.0.1", peer.Port, new TextLinoProtocol());

        Assert.Equal(reply.Count, client.Request([new LinoLink("ask")]).Count);

        var queued = client.ReceiveNotification(TimeSpan.FromSeconds(5));
        Assert.Equal(7u, LinksOperation.ParseNotification(queued!).Subscription);
    }

    [Fact]
    public void AGenericRemoteConnectsByEndPoint()
    {
        using var peer = new ScriptedPeer();
        using var remote = RemoteLinks<uint>.Connect(peer.EndPoint, new TextLinoProtocol());
        Assert.NotNull(remote);
    }

    [Fact]
    public void AWriteReplyWithoutTheChangeIsMalformed()
    {
        IReadOnlyList<LinoLink> changes = [new LinoLink(null, [Group(), Group()])];
        using var peer = new ScriptedPeer([changes], [changes]);
        using var remote = new RemoteLinks(LinksClient.Connect("127.0.0.1", peer.Port, new TextLinoProtocol()));

        Assert.Equal(LinoProtocolErrorKind.Malformed, Assert.Throws<LinoProtocolException>(() => remote.CreateTripletPoint()).Kind);
        Assert.Equal(LinoProtocolErrorKind.Malformed, Assert.Throws<LinoProtocolException>(() => remote.UpdateTriplet(5, 1, 1, 1)).Kind);
    }

    [Fact]
    public void TheNullAddressHoldsNoRemoteTriplet()
    {
        using var peer = new ScriptedPeer();
        using var remote = new RemoteLinks(LinksClient.Connect("127.0.0.1", peer.Port, new TextLinoProtocol()));
        Assert.Null(remote.GetTriplet(0));
    }

    [Fact]
    public void AChangeSideMayBeTheNamedTripletItself()
    {
        var side = new LinoLink("5", [new LinoLink("1"), new LinoLink("2"), new LinoLink("3")]);
        var change = Assert.Single(TripletsOperation.ParseChanges([new LinoLink(null, [Group(), side])]));
        Assert.Null(change.Before);
        Assert.Equal(5u, change.After!.Value.Index);
    }

    [Fact]
    public void SubscriptionsNeedAServerOnEveryAddressRange()
    {
        var notServer = (LinksOperation<uint> operation) =>
            Assert.Throws<InvalidOperationException>(() => operation.Execute(null!));
        notServer(new LinksOperation<uint>.Subscribe(LinksRestriction<uint>.Of(1u)));
        notServer(new LinksOperation<uint>.Unsubscribe(1u));
    }

    [Fact]
    public void AnUntypedLinkReadsLikeTheGenericOne()
    {
        var link = new DoubletLink(1, 2, 3);
        Assert.Equal(LinksOperation<uint>.LinkLino(link), LinksOperation.LinkLino(link));
        Assert.Equal(LinksOperation<uint>.Number(4), LinksOperation.Number(4));
    }

    [Fact]
    public void AClosedConnectionTakesNoReply()
    {
        using var owned = new MemoryStream();
        var connection = new LinksConnection(owned, 8);
        Assert.Empty(connection.Subscriptions);
        connection.Close();
        connection.Reply([new LinoLink("late")], default);
        Assert.True(connection.IsClosed);
    }
}
