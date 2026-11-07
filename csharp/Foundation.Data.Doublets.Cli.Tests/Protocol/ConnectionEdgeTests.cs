// What the two ends of a connection do when the stream under them fails.
using Foundation.Data.Doublets.Cli.Protocol;
using Foundation.Data.Doublets.Cli.Engine;
using Platform.Data.Doublets.Decorators;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;
using DoubletLink = Platform.Data.Doublets.Link<uint>;
using World = Foundation.Data.Doublets.Cli.Tests.PermissionsDecoratorTests.World;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

public sealed class ConnectionEdgeTests
{
    /// <summary>A stream that can time out and whose reads do what the test says.</summary>
    private sealed class FailingStream(Func<FailingStream, int> read) : MemoryStream
    {
        private bool _gone;

        public override bool CanTimeout => true;

        public override int ReadTimeout
        {
            get => 0;
            set => ObjectDisposedException.ThrowIf(_gone, this);
        }

        public override int Read(byte[] buffer, int offset, int count) => read(this);

        public override int Read(Span<byte> buffer) => read(this);

        public int Gone()
        {
            _gone = true;
            throw new ObjectDisposedException(nameof(FailingStream));
        }
    }

    private static LinoConnection Connection(Func<FailingStream, int> read) =>
        new(new FailingStream(read), new TextLinoProtocol());

    [Fact]
    public void AnInputErrorWhileWaitingIsAnIoFailureNotATimeout()
    {
        using var connection = Connection(_ => throw new IOException("the wire broke"));
        var error = Assert.Throws<LinoProtocolException>(() => connection.WaitForMessage(TimeSpan.FromSeconds(1)));
        Assert.Equal(LinoProtocolErrorKind.Io, error.Kind);
        Assert.Contains("the wire broke", error.Message, StringComparison.Ordinal);
    }

    [Fact]
    public void AStreamDisposedWhileWaitingIsAnIoFailure()
    {
        using var connection = Connection(stream => stream.Gone());
        var error = Assert.Throws<LinoProtocolException>(() => connection.WaitForMessage(Timeout.InfiniteTimeSpan));
        Assert.Equal(LinoProtocolErrorKind.Io, error.Kind);
    }

    [Fact]
    public void ARequestAnsweredByHangingUpIsMalformed()
    {
        using var connection = new LinoConnection(new MemoryStream(), new TextLinoProtocol());
        var error = Assert.Throws<LinoProtocolException>(() => connection.Request([new LinoLink("ask")]));
        Assert.Contains("closed before the reply", error.Message, StringComparison.Ordinal);
    }

    [Fact]
    public void AFailedWriteClosesTheConnectionAndIsReturned()
    {
        using var owned = new MemoryStream();
        var connection = new LinksConnection(owned, 8);
        connection.Reply([new LinoLink("answer")], default);
        var failure = new IOException("the wire broke");

        Assert.Same(failure, connection.WriteAll((_, _) => throw failure));
        Assert.True(connection.IsClosed);
    }

    [Fact]
    public void AClosedConnectionPushesNothing()
    {
        using var owned = new MemoryStream();
        var connection = new LinksConnection(owned, 8);
        var anything = new LinksRestriction([null, null, null]);
        connection.Subscribe(anything, default);
        connection.Subscribe(anything, default);
        connection.Close();

        connection.Notify([(default, new DoubletLink(1, 1, 1))], null);
        connection.NotifyTriplets([new TripletChange<uint>(null, new Triplet<uint>(1, 1, 1, 1))], null);

        Assert.Null(connection.WriteAll((_, _) => Assert.Fail("nothing may be written")));
    }

    /// <summary>A store that checks queries and whose name lookups fail.</summary>
    private sealed class FailingNames(INamedTypesLinks<uint> links) : LinksDecoratorBase<uint>(links), INamedTypesLinks<uint>
    {
        public QueryLimits QueryLimits { get; } = new();
        public string? GetName(uint id) => links.GetName(id);
        public uint SetName(uint id, string name) => links.SetName(id, name);
        public uint GetByName(string name) => throw new InvalidOperationException("the names are gone");
        public void RemoveName(uint id) => links.RemoveName(id);
    }

    private static IReadOnlyList<LinoLink> TooLarge(FailingNames links) =>
        LinksServer.QueryDocument(new string('x', links.QueryLimits.MaxInputBytes + 1));

    [Fact]
    public void AViewRequestThatThrowsIsUndoneAndRethrown()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var links = engine.Begin();
        var failing = new FailingNames(links);
        var policy = new PermissionPolicy<uint>(links, links);
        using var session = new LinksSessions(failing, new LinksServerOptions { Permissions = policy }).Open();

        var error = Assert.Throws<InvalidOperationException>(() => session.Execute([new LinoLink("get-by-name", [new LinoLink("x")])]));
        Assert.Equal("the names are gone", error.Message);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void AServerRefusesARequestOverItsLimitsAndWhenStopping(bool hasConnection)
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var links = engine.Begin();
        var failing = new FailingNames(links);
        using var session = new LinksSessions(failing).Open();
        var server = LinksServer.Bind("127.0.0.1", 0);
        using var owned = new MemoryStream();
        var connection = hasConnection ? new LinksConnection(owned, 8) : null;

        var rejected = server.Execute(session, TooLarge(failing), connection);
        Assert.Contains("byte limit", LinksServer.ErrorMessage(rejected)!, StringComparison.Ordinal);

        server.Shutdown();
        var stopping = server.Execute(session, [new LinoLink("count", [new LinoLink("*")])], connection);
        Assert.Equal("server is shutting down", LinksServer.ErrorMessage(stopping));

        if (connection is null) return;
        var written = new List<string?>();
        connection.WriteAll((document, _) =>
        {
            written.Add(LinksServer.ErrorMessage(document));
            if (written.Count == 2) connection.Close();
        });
        Assert.Equal(LinksServer.ErrorMessage(rejected), written[0]);
        Assert.Equal("server is shutting down", written[1]);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void AViewSessionAnswersAMalformedOperationWithAnError(bool triplets)
    {
        PermissionsDecoratorTests.WithWorld(w =>
        {
            using var session = new LinksSessions(w.Links, new LinksServerOptions { Triplets = triplets, Permissions = w.Policy }).Open();
            Assert.Contains("error:", session.Request("(count: (not-a-number))"), StringComparison.Ordinal);
            Assert.StartsWith("(count: ", session.Request("(count: (*))"), StringComparison.Ordinal);
        });
    }
}
