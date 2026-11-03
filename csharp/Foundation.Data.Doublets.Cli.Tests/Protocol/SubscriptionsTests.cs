using Foundation.Data.Doublets.Cli.Protocol;
using Link.Foundation.Links.Notation.Binary;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

/// <summary>
/// <c>(subscribe: …)</c>, <c>(notify: …)</c> and <c>(unsubscribe: …)</c>: a connection is told about the changes
/// other connections make to the links it subscribed to, and only those.
/// </summary>
public sealed class SubscriptionsTests
{
    private static readonly TimeSpan PushTimeout = TimeSpan.FromSeconds(10);
    private static readonly TimeSpan SilenceTimeout = TimeSpan.FromMilliseconds(200);

    /// <summary>One step of <c>docs/protocol/subscriptions.txt</c>: a request and its reply, or an expected push.</summary>
    private sealed record Step(char Connection, bool IsPush, string Message, string Expected);

    /// <summary>The steps of <c>docs/protocol/subscriptions.txt</c>, which the Rust and JavaScript tests replay too.</summary>
    private static List<Step> Conversation()
    {
        static bool Starts(string line) => line.Length > 2 && line[0] is 'A' or 'B' && line[1] is '>' or '<' && line[2] == ' ';
        var lines = File.ReadAllLines(Path.Combine(AppContext.BaseDirectory, "Protocol", "subscriptions.txt"))
            .Where(line => line.Length > 0 && !line.StartsWith('#'))
            .ToList();
        var steps = new List<Step>();
        for (var start = 0; start < lines.Count;)
        {
            Assert.True(Starts(lines[start]), $"expected a step, found '{lines[start]}'");
            var end = lines.FindIndex(start + 1, Starts);
            end = end < 0 ? lines.Count : end;
            var body = lines.Skip(start + 1).Take(end - start - 1);
            var isPush = lines[start][1] == '<';
            var message = lines[start][3..];
            steps.Add(new Step(lines[start][0], isPush, message, string.Join('\n', isPush ? body.Prepend(message) : body)));
            start = end;
        }
        return steps;
    }

    [Fact]
    public void ServersReplayTheSharedSubscriptionsConversationOverEveryProtocol()
    {
        var conversation = Conversation();
        Assert.Equal(24, conversation.Count);
        foreach (var protocol in RunningServer.Protocols())
        {
            using var server = new RunningServer();
            using var a = server.Client(protocol);
            using var b = server.Client(protocol);
            foreach (var step in conversation)
            {
                var client = step.Connection == 'A' ? a : b;
                if (step.IsPush)
                {
                    var push = client.ReceiveNotification(PushTimeout);
                    Assert.NotNull(push);
                    Assert.Equal(step.Expected, LinoFormat.FormatDocument(push));
                    continue;
                }
                if (step.Expected.StartsWith("! ", StringComparison.Ordinal))
                {
                    var error = Assert.Throws<LinoProtocolException>(() => client.QueryText(step.Message));
                    Assert.EndsWith(step.Expected[2..], error.Message);
                }
                else
                {
                    Assert.Equal((step.Message, step.Expected), (step.Message, client.QueryText(step.Message)));
                }
                // Every push the server made before this reply arrived before it, and the conversation lists none.
                Assert.Equal(0, client.PendingNotifications);
            }
            Assert.Null(a.ReceiveNotification(SilenceTimeout));
            Assert.Null(b.ReceiveNotification(SilenceTimeout));
        }
    }

    [Fact]
    public void ASubscriberIsToldAboutAnotherConnectionsWritesUntilItUnsubscribes()
    {
        foreach (var protocol in RunningServer.Protocols())
        {
            using var server = new RunningServer();
            using var subscriber = server.Remote(protocol);
            using var writer = server.Remote(protocol);

            var subscription = subscriber.Subscribe(LinksRestriction.Of(null, null, null));
            Assert.Equal(1u, subscription);
            writer.Create(null, null);

            Assert.Equal(
                new LinksNotification(1, new[] { (default(DoubletLink), new DoubletLink(1, 0, 0)) }),
                subscriber.ReceiveNotification(PushTimeout));
            Assert.Null(subscriber.ReceiveNotification(SilenceTimeout));

            subscriber.Unsubscribe(subscription);
            writer.Create(null, null);
            Assert.Equal(2u, subscriber.Count(null));
            Assert.Null(subscriber.ReceiveNotification(SilenceTimeout));
        }
    }

    [Fact]
    public void ClosingAConnectionDropsItsSubscriptions()
    {
        using var server = new RunningServer();
        using (var subscriber = server.Remote(new TextLinoProtocol()))
        {
            subscriber.Subscribe(LinksRestriction.All);
            WaitFor(() => server.Server.SessionCount == 1);
        }
        WaitFor(() => server.Server.SessionCount == 0);
        using var writer = server.Remote(new TextLinoProtocol());
        writer.Create(null, null);
        Assert.Equal(1u, writer.Count(null));
    }

    [Fact]
    public void ASubscriberIsPushedOnlyWhatItsSubjectMayRead()
    {
        // Issue #149: with permissions every connection is a session of its subject, and its pushes are filtered by them.
        PermissionsDecoratorTests.WithWorld(w =>
        {
            var server = LinksServer.Bind("127.0.0.1", 0, new LinksServerOptions
            {
                Permissions = w.Policy,
                Authenticate = token => w.V.TryGetValue(token, out var subject) ? subject : null,
            });
            Exception? failure = null;
            var thread = new Thread(() =>
            {
                try
                {
                    server.Serve(w.Links);
                }
                catch (Exception error)
                {
                    failure = error;
                }
            })
            { IsBackground = true };
            thread.Start();
            try
            {
                var (alice, boss, note, secret) = (w.V["alice"], w.V["boss"], w.V["note"], w.V["secret"]);
                var aliceClient = LinksClient.Connect("127.0.0.1", server.LocalEndPoint.Port, new TextLinoProtocol());
                Assert.Equal(alice, aliceClient.Authenticate("alice"));
                using var subscriber = new RemoteLinks(aliceClient);
                using var writer = LinksClient.Connect("127.0.0.1", server.LocalEndPoint.Port, new TextLinoProtocol());
                Assert.Equal(boss, writer.Authenticate("boss"));
                subscriber.Subscribe(LinksRestriction.All);

                // alice may not read the secret: a change she cannot see is not pushed to her at all...
                writer.QueryText($"(update: ({secret} {w.T["Secret"]} {alice}))");
                // ...while one to a note she may read is.
                writer.QueryText($"(update: ({note} {w.T["Note"]} {alice}))");

                Assert.Equal(
                    new LinksNotification(1, new[] { (new DoubletLink(note, w.T["Note"], note), new DoubletLink(note, w.T["Note"], alice)) }),
                    subscriber.ReceiveNotification(PushTimeout));
                Assert.Null(subscriber.ReceiveNotification(SilenceTimeout));
            }
            finally
            {
                server.Shutdown();
                Assert.True(thread.Join(TimeSpan.FromSeconds(30)), "the server did not stop");
            }
            Assert.Null(failure);
        });
    }

    [Fact]
    public void ASubscriberSeesOnlyWhatItMayRead()
    {
        // A link whose source is 2 is hidden: one that becomes hidden is gone, one that stops being hidden is new.
        using var server = new RunningServer(new LinksServerOptions { Readable = link => link.Source != 2 });
        using var subscriber = server.Remote(new TextLinoProtocol());
        using var writer = server.Remote(new TextLinoProtocol());
        subscriber.Subscribe(LinksRestriction.All);

        writer.Create(null, null);
        writer.Update(new uint[] { 1 }, new uint[] { 1, 2, 2 }, null);
        writer.Update(new uint[] { 1 }, new uint[] { 1, 2, 1 }, null);
        writer.Update(new uint[] { 1 }, new uint[] { 1, 1, 1 }, null);

        Assert.Equal(new LinksNotification(1, new[] { (default(DoubletLink), new DoubletLink(1, 0, 0)) }), subscriber.ReceiveNotification(PushTimeout));
        Assert.Equal(new LinksNotification(1, new[] { (new DoubletLink(1, 0, 0), default(DoubletLink)) }), subscriber.ReceiveNotification(PushTimeout));
        // A change between two hidden states is not pushed at all.
        Assert.Equal(new LinksNotification(1, new[] { (default(DoubletLink), new DoubletLink(1, 1, 1)) }), subscriber.ReceiveNotification(PushTimeout));
        Assert.Null(subscriber.ReceiveNotification(SilenceTimeout));
    }

    [Fact]
    public void ASubscriberThatFallsTooFarBehindIsDisconnected()
    {
        using var server = new RunningServer(new LinksServerOptions { MaxPendingNotifications = 0 });
        using var subscriber = server.Client(new TextLinoProtocol());
        using var writer = server.Client(new TextLinoProtocol());
        subscriber.Query("(subscribe: ())");

        writer.Query("(create: (0 0))");

        Assert.ThrowsAny<LinoProtocolException>(() =>
        {
            subscriber.ReceiveNotification(PushTimeout);
            subscriber.Query("(count: ())");
        });
        WaitFor(() => server.Server.SessionCount == 1);
        Assert.Equal("(count: 1)", writer.QueryText("(count: ())"));
    }

    [Fact]
    public void SubscriptionsNeedAServer()
    {
        var databaseFilename = Path.GetTempFileName();
        try
        {
            using var links = new NamedTypesDecorator<uint>(databaseFilename);
            var reply = LinksServer.ExecuteRequest(links, LinoFormat.ParseDocument("(subscribe: ())"));
            Assert.Equal("subscriptions belong to a connection to a links server", LinksServer.ErrorMessage(reply));
        }
        finally
        {
            File.Delete(databaseFilename);
            File.Delete(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(databaseFilename));
        }
        var document = new LinksOperation.Subscribe(LinksRestriction.All).ToDocument();
        Assert.Throws<InvalidOperationException>(() => new LinksOperation.Subscribe(LinksRestriction.All).Execute(null!));
        Assert.Throws<InvalidOperationException>(() => new LinksOperation.Unsubscribe(1).Execute(null!));
        Assert.Equal("(subscribe: ())", LinoFormat.FormatDocument(document));
    }

    [Theory]
    [InlineData("(notify: 1)", true)]
    [InlineData("(notify: 2)\n() ((1: 0 0))", true)]
    [InlineData("(notify: 1 2)", false)]
    [InlineData("(notify: x)", false)]
    [InlineData("(notify: (1 2))", false)]
    [InlineData("(1: 2 2)", false)]
    [InlineData("(count: 1)", false)]
    [InlineData("", false)]
    public void OnlyNotifyDocumentsArePushes(string document, bool isPush) =>
        Assert.Equal(isPush, LinksOperation.IsNotification(LinoFormat.ParseDocument(document)));

    [Fact]
    public void NotificationsRoundTripThroughTheirDocuments()
    {
        var changes = new[]
        {
            (default(DoubletLink), new DoubletLink(1, 0, 0)),
            (new DoubletLink(2, 1, 1), new DoubletLink(2, 1, 2)),
            (new DoubletLink(3, 3, 3), default(DoubletLink)),
        };
        var document = LinksOperation.NotificationDocument(7, changes);

        Assert.Equal("(notify: 7)\n() ((1: 0 0))\n((2: 1 1)) ((2: 1 2))\n((3: 3 3)) ()", LinoFormat.FormatDocument(document));
        Assert.Equal(new LinksNotification(7, changes), LinksOperation.ParseNotification(LinoFormat.ParseDocument(LinoFormat.FormatDocument(document))));
        Assert.Equal(3u, LinksOperation.ParseSubscription(LinksOperation.SubscriptionDocument(3)));
        Assert.Throws<LinoProtocolException>(() => LinksOperation.ParseNotification(LinoFormat.ParseDocument("(count: 1)")));
        Assert.Throws<LinoProtocolException>(() => LinksOperation.ParseSubscription(LinoFormat.ParseDocument("(count: 1)")));
    }

    private static void WaitFor(Func<bool> condition)
    {
        var deadline = DateTime.UtcNow + PushTimeout;
        while (!condition())
        {
            Assert.True(DateTime.UtcNow < deadline, "the condition never held");
            Thread.Sleep(10);
        }
    }
}
