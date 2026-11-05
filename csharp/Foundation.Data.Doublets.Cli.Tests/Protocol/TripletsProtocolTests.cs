using System.Net.Sockets;
using Foundation.Data.Doublets.Cli.Protocol;
using Link.Foundation.Links.Notation.Binary;
using Platform.Data;
using static Foundation.Data.Doublets.Cli.Tests.PermissionsDecoratorTests;
using Triplet = Foundation.Data.Doublets.Cli.Triplet<uint>;
using TripletChange = Foundation.Data.Doublets.Cli.TripletChange<uint>;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

/// <summary>
/// Triplets over the text and binary protocol (issue #152): a server in triplet mode answers with triplets, a server
/// without it answers as it always did, and <see cref="RemoteLinks"/> is a drop-in replacement for a local store of
/// triplets.
/// </summary>
public sealed class TripletsProtocolTests
{
    private static readonly TimeSpan PushTimeout = TimeSpan.FromSeconds(10);
    private static readonly TimeSpan SilenceTimeout = TimeSpan.FromMilliseconds(200);

    private static RunningServer TripletServer() => new(new LinksServerOptions { Triplets = true });

    /// <summary>One step of <c>docs/protocol/triplets.txt</c>: a request and its reply, or an expected push.</summary>
    private sealed record Step(char Connection, bool IsPush, string Message, string Expected);

    /// <summary>The steps of <c>docs/protocol/triplets.txt</c>, which the Rust and JavaScript tests replay too.</summary>
    private static List<Step> Conversation()
    {
        static bool Starts(string line) => line.Length > 2 && line[0] is 'A' or 'B' && line[1] is '>' or '<' && line[2] == ' ';
        var lines = File.ReadAllLines(Path.Combine(AppContext.BaseDirectory, "Protocol", "triplets.txt"))
            .Where(line => line.Length > 0 && !line.StartsWith('#')).ToList();
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

    private static string Canonical(IReadOnlyList<Link.Foundation.Links.Notation.Link<string>> document) =>
        LinoFormat.FormatDocument(TripletsOperation.Canonical(document));

    private static IReadOnlyList<Link.Foundation.Links.Notation.Link<string>> Parse(string text) => LinoFormat.ParseDocument(text);

    [Fact]
    public void ServersReplayTheSharedTripletsConversationOverEveryProtocol()
    {
        var conversation = Conversation();
        Assert.Equal(48, conversation.Count);
        foreach (var (first, second) in RunningServer.Protocols().Zip(RunningServer.Protocols()))
        {
            using var server = TripletServer();
            using var a = server.Client(first);
            using var b = server.Client(second);
            foreach (var step in conversation)
            {
                var client = step.Connection == 'A' ? a : b;
                if (step.IsPush)
                {
                    var push = client.ReceiveNotification(PushTimeout);
                    Assert.NotNull(push);
                    Assert.Equal(step.Expected, Canonical(push));
                    continue;
                }
                if (step.Expected.StartsWith("! ", StringComparison.Ordinal))
                {
                    var error = Assert.Throws<LinoProtocolException>(() => client.Query(step.Message));
                    Assert.True(error.Message.EndsWith(step.Expected[2..], StringComparison.Ordinal), $"{step.Message}: {error.Message}");
                }
                else
                {
                    Assert.Equal((step.Message, step.Expected), (step.Message, Canonical(client.Query(step.Message))));
                }
                Assert.Equal(0, client.PendingNotifications);
            }
            Assert.Null(a.ReceiveNotification(SilenceTimeout));
            Assert.Null(b.ReceiveNotification(SilenceTimeout));
        }
    }

    [Fact]
    public void AServerWithoutTripletModeAnswersAsItAlwaysDid()
    {
        using var server = new RunningServer();
        using var client = server.Client(new TextLinoProtocol());

        Assert.Equal("() ((1: 0 0))", client.QueryText("(create: (0 0))"));
        Assert.Equal("(1: 0 0)", client.QueryText("(each: (* * *))"));
        foreach (var request in new[] { "(create: (1 1 1))", "(update: (1 1 1 1))", "(each: (1 2 3 4))" })
        {
            Assert.Throws<LinoProtocolException>(() => client.QueryText(request));
        }
        // It created no markers either.
        Assert.Equal("(count: 1)", client.QueryText("(count: ())"));
    }

    /// <summary>A raw connection, to see the packet the server writes.</summary>
    private static LinksPacket RawRequest(Stream stream, PacketReader reader, string request, BinaryLinoOptions options)
    {
        stream.Write(new BinaryLinoProtocol(options).Encode(Parse(request)));
        stream.Flush();
        var reply = LinksPacket.ReadFrom(reader, DecodeLimits.Default);
        Assert.NotNull(reply);
        return reply;
    }

    [Fact]
    public void AClientThatAsksForTwoToThreeGetsEachTripletAsOneLinkOfThreeReferences()
    {
        using var server = TripletServer();
        using var text = server.Client(new TextLinoProtocol());
        foreach (var request in new[] { "(create: (0 0 0))", "(create: (0 0 0))", "(create: (3 0 0))", "(create: (3 0 0))", "(create: (4 5 6))" })
        {
            text.Query(request);
        }
        var options = new BinaryLinoOptions { ExternalReferences = true, Arity = ArityRange.Between(2, 3), PackedWidths = false };
        using var connection = new TcpClient("127.0.0.1", server.Port) { ReceiveTimeout = 10_000 };
        var stream = connection.GetStream();
        var reader = new PacketReader(stream);

        var reply = RawRequest(stream, reader, "(each: (* 4 5 6))", options);

        var expected = new[] { PacketReference.External(4), PacketReference.External(5), PacketReference.External(6) };
        Assert.Contains(reply.Links(), link => link.References.SequenceEqual(expected));
        Assert.Equal("(9: (4 5 6))", LinoFormat.FormatDocument(LinoMapping.DecodeDocument(reply, DecodeLimits.Default)));
        Assert.All(reply.Links(), link => Assert.InRange(link.References.Length, 2, 3));

        // Doublets alone keep the form as written, spelled in pairs.
        var doublets = RawRequest(stream, reader, "(each: (* 4 5 6))", options with { Arity = ArityRange.Doublets });
        Assert.All(doublets.Links(), link => Assert.Equal(2, link.References.Length));
        Assert.Equal("(9: 4 5 6)", LinoFormat.FormatDocument(LinoMapping.DecodeDocument(doublets, DecodeLimits.Default)));
    }

    [Fact]
    public void PackingChangesOnlyTheTripletsOfAFormatThatHoldsThreeButNotFive()
    {
        var document = Parse("((1: 2 3 4)) ((1: 2 3 5))\n(count: 1)\n(name: a)");
        static MessageFormat With(ArityRange arity) => MessageFormat.Binary(new BinaryLinoOptions { Arity = arity });

        var packed = TripletsOperation.Pack(document, With(ArityRange.Between(2, 3)));
        Assert.Equal("((1: (2 3 4))) ((1: (2 3 5)))\n(count: 1)\n(name: a)", LinoFormat.FormatDocument(packed));
        Assert.Equal(document, TripletsOperation.Canonical(packed));

        foreach (var format in new[] { MessageFormat.Text, With(ArityRange.Doublets), With(ArityRange.AtLeast(1)) })
        {
            Assert.Equal(document, TripletsOperation.Pack(document, format));
        }
    }

    [Fact]
    public void EveryParserReadsBothFormsOfATriplet()
    {
        var triplet = new Triplet(1, 2, 3, 4);
        foreach (var text in new[] { "(1: 2 3 4)", "(1: (2 3 4))" })
        {
            Assert.Equal(new[] { triplet }, TripletsOperation.ParseTriplets(Parse(text)));
        }
        foreach (var text in new[] { "((1: 2 3 4)) ()", "((1: (2 3 4))) ()" })
        {
            Assert.Equal(new[] { new TripletChange(triplet, null) }, TripletsOperation.ParseChanges(Parse(text)));
        }
        var notification = new TripletsNotification(3, new[] { new TripletChange(null, triplet) });
        var document = TripletsOperation.NotificationDocument(3, notification.Changes);
        Assert.Equal("(notify: 3)\n() ((1: 2 3 4))", LinoFormat.FormatDocument(document));
        Assert.Equal(notification, TripletsOperation.ParseNotification(document));
    }

    [Fact]
    public void OperationsRoundTripThroughTheirDocuments()
    {
        foreach (var (operation, text) in new (TripletsOperation, string)[]
        {
            (new TripletsOperation.Count(LinksRestriction.All), "(count: ())"),
            (new TripletsOperation.Each(LinksRestriction.Of(null, 4, null, 0)), "(each: (* 4 * 0))"),
            (new TripletsOperation.Create(4, 5, 6), "(create: (4 5 6))"),
            (new TripletsOperation.Update(9, 4, 5, 0), "(update: (9 4 5 0))"),
            (new TripletsOperation.Delete(9), "(delete: 9)"),
            (new TripletsOperation.Subscribe(LinksRestriction.Of(null, 4)), "(subscribe: (* 4))"),
        })
        {
            Assert.Equal(text, LinoFormat.FormatDocument(operation.ToDocument()));
            Assert.Equal(operation, TripletsOperation.FromDocument(Parse(text)));
        }
        foreach (var other in new[] { "(get-name: 1)", "() ((1 2 3))", "(1: 2 3 4)", "" })
        {
            Assert.Null(TripletsOperation.FromDocument(Parse(other)));
        }
    }

    /// <summary>Runs <paramref name="test"/> with triplets over a new local store.</summary>
    private static void WithLocalTriplets(Action<TripletsOverDoublets<uint>> test)
    {
        var database = Path.GetTempFileName();
        try
        {
            using var store = new NamedTypesDecorator<uint>(database);
            test(new TripletsOverDoublets<uint>(store));
        }
        finally
        {
            File.Delete(database);
            File.Delete(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(database));
        }
    }

    [Fact]
    public void ExecuteRequestAnswersLikeAServerInTripletMode()
    {
        WithLocalTriplets(triplets =>
        {
            string Request(string text) => LinoFormat.FormatDocument(TripletsOperation.ExecuteRequest(triplets, Parse(text)));

            Assert.Equal("", Request(""));
            Assert.Equal("() ((3: 3 0 0))", Request("(create: (0 0 0))"));
            Assert.Equal("() ((4: 3 0 0))", Request("(create: (3 0 0))"));
            Assert.Equal("(3: 3 0 0)\n(4: 3 0 0)", Request(""));
            Assert.Equal("(error: 'subscriptions belong to a connection to a links server')", Request("(subscribe: ())"));
            Assert.Equal("((3: 3 0 0)) ()\n((4: 3 0 0)) ()", Request("(delete: 3)"));
        });
    }

    private static string Describe(IEnumerable<Triplet> triplets) => string.Join(' ', triplets);

    private static string ErrorOf(Action action)
    {
        var error = Assert.ThrowsAny<Exception>(action);
        return error.Message;
    }

    /// <summary>Every call of <see cref="ITriplets{TLinkAddress}"/> once, returning what the store answered.</summary>
    private static List<string> Exercise(ITriplets<uint> store)
    {
        var person = store.CreateTripletPoint();
        var knows = store.CreateTripletPoint();
        var alice = store.CreateTriplet(person, 0, 0);
        var bob = store.CreateTriplet(person, 0, 0);
        var first = store.CreateTriplet(knows, alice, bob);
        var second = store.CreateTriplet(knows, alice, bob);
        store.CreateTripletPointAt(77);
        return new List<string>
        {
            $"{person} {knows} {alice} {bob} {first} {second}",
            $"{store.GetTriplet(first)}",
            $"{store.GetTriplet(store.Markers.Pair)}",
            $"{store.IsTriplet(first)}",
            $"{store.IsTriplet(store.Markers.Pair)}",
            $"{store.CountTriplets(TripletQuery<uint>.Any)}",
            Describe(store.EachTriplet(TripletQuery<uint>.ByType(knows))),
            Describe(store.EachTriplet(TripletQuery<uint>.ByFrom(alice))),
            Describe(store.EachTriplet(TripletQuery<uint>.ByTo(bob))),
            $"{store.UpdateTriplet(first, knows, bob, alice)}",
            $"{store.GetTriplet(first)}",
            Describe(store.DeleteTriplet(alice)),
            Describe(store.EachTriplet(TripletQuery<uint>.Any)),
            ErrorOf(() => store.CreateTriplet(99, 0, 0)),
            ErrorOf(() => store.CreateTriplet(0, person, person)),
            ErrorOf(() => store.UpdateTriplet(99, knows, 0, 0)),
            ErrorOf(() => store.DeleteTriplet(99)),
        };
    }

    [Fact]
    public void EveryTripletsCallMatchesALocalStore()
    {
        List<string> expected = new();
        WithLocalTriplets(local => expected = Exercise(local));

        foreach (var protocol in RunningServer.Protocols())
        {
            using var server = TripletServer();
            using var remote = server.Remote(protocol);
            var answers = Exercise(remote);
            Assert.Equal(expected.Count, answers.Count);
            foreach (var (answer, wanted) in answers.Zip(expected))
            {
                Assert.True(answer == wanted || answer.EndsWith(wanted, StringComparison.Ordinal), $"{answer} != {wanted}");
            }
        }
    }

    [Fact]
    public void TheQueryProcessorGivesTheSameResultsOnARemoteStore()
    {
        var queries = new[]
        {
            "() ((1 2 3))",
            "() ((person: person 0 0))",
            "() ((knows: knows 0 0))",
            "() ((alice: person 0 0))",
            "() ((bob: person 0 0))",
            "() ((knows alice bob))",
            "((($i: knows $f $t))) (($i: knows $t $f))",
            "(($i: person 0 0)) ()",
        };
        List<string> Run(ITriplets<uint> store) => queries.Select(query =>
        {
            try
            {
                return $"{query} {string.Join(' ', TripletsQueryProcessor.ProcessQuery(store, query))}";
            }
            catch (Exception error) when (error is ArgumentException or InvalidOperationException or LinoProtocolException)
            {
                return $"{query} {error.Message.Replace(RemoteErrorPrefix, "", StringComparison.Ordinal)}";
            }
        }).ToList();

        List<string> expected = new();
        List<Triplet> expectedTriplets = new();
        WithLocalTriplets(local =>
        {
            expected = Run(local);
            expectedTriplets = local.EachTriplet(TripletQuery<uint>.Any).ToList();
        });

        using var server = TripletServer();
        using var remote = server.Remote(new TextLinoProtocol());
        Assert.Equal(expected, Run(remote));
        Assert.Equal(expectedTriplets, remote.EachTriplet(TripletQuery<uint>.Any));
    }

    private const string RemoteErrorPrefix = "Remote error: ";

    [Fact]
    public void NamesStayAvailableThroughRemoteLinks()
    {
        using var server = TripletServer();
        using var remote = server.Remote(new TextLinoProtocol());
        var point = remote.CreateTripletPoint();

        remote.SetName(point, "person");

        Assert.Equal(point, remote.GetByName("person"));
        Assert.Equal("person", remote.GetName(point));
        Assert.True(remote.Exists(point));
        Assert.False(remote.Exists(999u));
        Assert.Equal(1u, remote.Markers.Pair);
        Assert.Equal(2u, remote.Markers.Nothing);
    }

    [Fact]
    public void ARemoteSubscriberIsToldAboutAnotherConnectionsTriplets()
    {
        foreach (var (first, second) in RunningServer.Protocols().Zip(RunningServer.Protocols()))
        {
            using var server = TripletServer();
            using var subscriber = server.Remote(first);
            using var writer = server.Remote(second);
            var person = writer.CreateTripletPoint();
            var knows = writer.CreateTripletPoint();

            var subscription = subscriber.SubscribeTriplets(LinksRestriction.Of(null, person));
            Assert.Equal(1u, subscription);
            var alice = writer.CreateTriplet(person, 0, 0);
            // A triplet of another type is not pushed.
            writer.CreateTriplet(knows, alice, alice);

            Assert.Equal(
                new TripletsNotification(subscription, new[] { new TripletChange(null, new Triplet(alice, person, 0, 0)) }),
                subscriber.ReceiveTripletNotification(PushTimeout));
            Assert.Null(subscriber.ReceiveTripletNotification(SilenceTimeout));

            subscriber.Unsubscribe(subscription);
            writer.CreateTriplet(person, 0, 0);
            Assert.Null(subscriber.ReceiveTripletNotification(SilenceTimeout));
        }
    }

    /// <summary>
    /// Runs <paramref name="test"/> with a server in triplet mode over the world of the permission tests, whose values
    /// are triplets: <c>(note: Note note)</c> reads <c>(note: Note 0 0)</c>.
    /// </summary>
    private static void WithPermittedServer(Action<World, int> test)
    {
        WithWorld(w =>
        {
            var server = LinksServer.Bind("127.0.0.1", 0, new LinksServerOptions
            {
                Triplets = true,
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
                test(w, server.LocalEndPoint.Port);
            }
            finally
            {
                server.Shutdown();
                Assert.True(thread.Join(TimeSpan.FromSeconds(30)), "the server did not stop");
            }
            Assert.Null(failure);
        });
    }

    private static LinksClient Authenticated(World w, int port, string name)
    {
        var client = LinksClient.Connect("127.0.0.1", port, new TextLinoProtocol());
        Assert.Equal(w.V[name], client.Authenticate(name));
        return client;
    }

    private static void AssertRefused(LinksClient client, string request, string message)
    {
        var error = Assert.Throws<LinoProtocolException>(() => client.QueryText(request));
        Assert.True(error.Message.Contains(message, StringComparison.Ordinal), $"{request}: {error.Message}");
    }

    private static Triplet Created(IReadOnlyList<Link.Foundation.Links.Notation.Link<string>> reply) =>
        TripletsOperation.ParseChanges(reply)[0].After!.Value;

    [Fact]
    public void ASubjectReadsAndWritesOnlyTheTripletsItMay()
    {
        WithPermittedServer((w, port) =>
        {
            var (t, v) = (w.T, w.V);
            var (note, shared, secret) = (v["note"], v["shared"], v["secret"]);
            using var alice = Authenticated(w, port, "alice");
            using var boss = Authenticated(w, port, "boss");

            // boss writes anything, so reads anything.
            Assert.Equal($"({secret}: {t["Secret"]} 0 0)", boss.QueryText($"(each: ({secret}))"));
            // alice reads the notes she may write and what is public; a secret is not there at all.
            Assert.Equal("", alice.QueryText($"(each: ({secret}))"));
            Assert.Equal("", alice.QueryText($"(each: (* {t["Secret"]}))"));
            Assert.Equal($"({note}: {t["Note"]} 0 0)", alice.QueryText($"(each: ({note}))"));
            Assert.Equal($"({shared}: {t["Shared"]} 0 0)", alice.QueryText($"(each: ({shared}))"));
            Assert.Equal("(count: 0)", alice.QueryText($"(count: ({secret}))"));

            // alice may write notes: creating one, and turning it to point at another.
            var created = Created(alice.Query($"(create: ({t["Note"]} {note} {shared}))"));
            Assert.Equal(note, created.From);
            Assert.Equal(
                $"(({created.Index}: {t["Note"]} {note} {shared})) (({created.Index}: {t["Note"]} {shared} {note}))",
                alice.QueryText($"(update: ({created.Index} {t["Note"]} {shared} {note}))"));

            // She may not make one of the secrets, nor make anything secret.
            AssertRefused(alice, $"(create: ({t["Secret"]} 0 0))", "Forbidden");
            AssertRefused(alice, $"(update: ({created.Index} {t["Secret"]} 0 0))", "Forbidden");
            // What she cannot read is not a triplet to her.
            AssertRefused(alice, $"(delete: {secret})", "is not a triplet");
            AssertRefused(alice, $"(update: ({secret} {t["Note"]} 0 0))", "is not a triplet");
            AssertRefused(alice, $"(create: ({t["Note"]} {secret} 0))", "is not a triplet");

            // A triplet of boss that uses the note she may delete hides in the cascade: the whole delete is refused,
            // and nothing changes.
            var hidden = Created(boss.Query($"(create: ({t["Secret"]} {note} {note}))"));
            AssertRefused(alice, $"(delete: {note})", "Forbidden");
            Assert.Equal($"({hidden.Index}: {t["Secret"]} {note} {note})", boss.QueryText($"(each: ({hidden.Index}))"));
            Assert.Equal($"({note}: {t["Note"]} 0 0)", boss.QueryText($"(each: ({note}))"));

            // Boss may.
            Assert.StartsWith($"(({note}: {t["Note"]} 0 0)) ()", boss.QueryText($"(delete: {note})"), StringComparison.Ordinal);
        });
    }

    [Fact]
    public void ASubjectKeepsThePointsItCreatedAndAGuestWritesNothing()
    {
        WithPermittedServer((w, port) =>
        {
            using var alice = Authenticated(w, port, "alice");
            using var boss = Authenticated(w, port, "boss");
            var note = w.V["note"];

            // A point has no type to be judged by: it is the subject's own, until it gets a type.
            var point = Created(alice.Query("(create: (0 0 0))"));
            Assert.Equal(point.Index, point.Type);
            var index = point.Index;
            Assert.Equal($"({index}: {index} 0 0)", alice.QueryText($"(each: ({index}))"));
            boss.QueryText($"(update: ({index} {index} {note} 0))");
            Assert.Equal("", alice.QueryText($"(each: ({index}))"));

            // A substitution query reserves the point at the address it names, and reads what it refers to: the type
            // she may write yet not read is not there for it, unlike for a create.
            Assert.Equal("() ((900: 900 0 0))", alice.QueryText("() ((900: 900 0 0))"));
            AssertRefused(alice, $"() ((901: {w.T["Note"]} 0 0))", "is not a triplet");
            Assert.Equal("((900: 900 0 0)) ()", alice.QueryText("(delete: 900)"));

            using var guest = LinksClient.Connect("127.0.0.1", port, new TextLinoProtocol());
            foreach (var request in new[] { "(create: (0 0 0))", $"(create: ({w.T["Note"]} 0 0))", "() ((901: 901 0 0))" })
            {
                AssertRefused(guest, request, "Forbidden");
            }
            Assert.Equal("(count: 0)", guest.QueryText("(count: ())"));
        });
    }

    [Fact]
    public void ASubscriberIsPushedOnlyTheTripletsItsSubjectMayRead()
    {
        WithPermittedServer((w, port) =>
        {
            var (t, v) = (w.T, w.V);
            var (alice, note, secret) = (v["alice"], v["note"], v["secret"]);
            using var subscriber = new RemoteLinks(Authenticated(w, port, "alice"));
            using var writer = Authenticated(w, port, "boss");
            subscriber.SubscribeTriplets(LinksRestriction.All);

            // A change she cannot see is not pushed to her at all...
            writer.Query($"(update: ({secret} {t["Secret"]} {alice} 0))");
            // ...while one to a note she may read is.
            writer.Query($"(update: ({note} {t["Note"]} {alice} 0))");

            Assert.Equal(
                new TripletsNotification(1, new[] { new TripletChange(new Triplet(note, t["Note"], 0, 0), new Triplet(note, t["Note"], alice, 0)) }),
                subscriber.ReceiveTripletNotification(PushTimeout));
            Assert.Null(subscriber.ReceiveTripletNotification(SilenceTimeout));
        });
    }

    [Fact]
    public void ABinaryClientOfTwoToThreeGetsPackedPushesToo()
    {
        using var server = TripletServer();
        using var subscriber = server.Client(new BinaryLinoProtocol(new BinaryLinoOptions { Arity = ArityRange.Between(2, 3) }));
        using var writer = server.Client(new TextLinoProtocol());
        subscriber.Query("(subscribe: ())");
        writer.Query("(create: (0 0 0))");

        var push = subscriber.ReceiveNotification(PushTimeout);

        Assert.NotNull(push);
        Assert.Equal("(notify: 1)\n() ((3: (3 0 0)))", LinoFormat.FormatDocument(push));
        Assert.Equal(new[] { new TripletChange(null, new Triplet(3, 3, 0, 0)) }, TripletsOperation.ParseNotification(push).Changes);
    }

    [Fact]
    public void TripletModeKeepsAnExistingStoreOfDoubletsReadable()
    {
        // The server only adds the markers; a store that had triplets keeps them.
        var database = Path.GetTempFileName();
        try
        {
            Triplet existing;
            using (var store = new NamedTypesDecorator<uint>(database))
            {
                var triplets = new TripletsOverDoublets<uint>(store);
                var point = triplets.CreateTripletPoint();
                existing = new Triplet(triplets.CreateTriplet(point, 0, 0), point, 0, 0);
            }
            var server = LinksServer.Bind("127.0.0.1", 0, new LinksServerOptions { Triplets = true });
            Exception? failure = null;
            var thread = new Thread(() =>
            {
                try
                {
                    using var store = new NamedTypesDecorator<uint>(database);
                    server.Serve(store);
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
                using var remote = new RemoteLinks(LinksClient.Connect("127.0.0.1", server.LocalEndPoint.Port, new TextLinoProtocol()));
                Assert.Contains(existing, remote.TripletsMatching(LinksRestriction.Of(existing.Index)));
            }
            finally
            {
                server.Shutdown();
                Assert.True(thread.Join(TimeSpan.FromSeconds(30)), "the server did not stop");
            }
            Assert.Null(failure);
        }
        finally
        {
            File.Delete(database);
            File.Delete(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(database));
        }
    }
}
