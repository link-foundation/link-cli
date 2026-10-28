using System.Net.Sockets;
using Foundation.Data.Doublets.Cli.Protocol;
using Foundation.Data.Doublets.Cli.Tests.Protocol;
using Link.Foundation.Links.Notation;
using Link.Foundation.Links.Notation.Binary;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Tests;

public sealed partial class CausalQueryTests
{
    private static IReadOnlyList<LinoLink> RoundTrip(ILinoProtocol protocol, IReadOnlyList<LinoLink> document)
    {
        using var wire = new MemoryStream();
        protocol.WriteDocument(wire, document); wire.Position = 0;
        return protocol.ReadDocument(new PacketReader(wire))!;
    }

    [Theory]
    [MemberData(nameof(Vectors))]
    public void SharedVectorsUseStandardLosslessDispatchThroughEveryCodec(string name, string json)
    {
        foreach (var protocol in RunningServer.Protocols())
            CheckVector(name, json, (links, query) =>
            {
                var reply = LinksServer.ExecuteRequest(links, RoundTrip(protocol, LinksServer.QueryDocument(query)));
                if (LinksServer.ErrorMessage(RoundTrip(protocol, reply)) is { } message)
                    throw new InvalidOperationException(message);
            });
    }

    [Fact]
    public void StandardWideOperationsValidateBeforeCreatingAndKeepEveryIdentityConcrete()
    {
        var store = new Store();
        Batch(store).Write(Token(10), 0, [new(ulong.MaxValue, [], new(0, ulong.MaxValue, null))]);
        var persisted = Bytes(store);
        using var tx = Session(store);
        var links = tx.QueryLinks;
        IReadOnlyList<LinoLink> Run(LinksOperation<UInt128> operation) =>
            LinksServer.ExecuteRequest(links, operation.ToDocument());
        Assert.NotNull(LinksServer.ErrorMessage(Run(new LinksOperation<UInt128>.Create((UInt128)ulong.MaxValue + 1, 0))));
        Assert.Single(tx.Rows());
        var change = Assert.Single(LinksOperation<UInt128>.ParseChanges(Run(new LinksOperation<UInt128>.Create(ulong.MaxValue, 0))));
        Assert.Equal(UInt128.One, change.After.Index);
        Assert.Equal((UInt128)ulong.MaxValue, change.After.Source);
        Assert.Equal((UInt128)1, LinksOperation<UInt128>.ParseCount(Run(new LinksOperation<UInt128>.Count(LinksRestriction<UInt128>.Of(ulong.MaxValue)))));
        Assert.Equal((UInt128)2, LinksOperation<UInt128>.ParseCount(Run(new LinksOperation<UInt128>.Count(LinksRestriction<UInt128>.All))));
        Assert.Equal(2, LinksOperation<UInt128>.ParseLinks(Run(new LinksOperation<UInt128>.Each(LinksRestriction<UInt128>.All))).Count);
        const string name = "18446744073709551615\0雪\n";
        Assert.Equal(UInt128.One, LinksOperation<UInt128>.ParseLinkReply(Run(new LinksOperation<UInt128>.SetName(1, name))));
        Assert.Equal(name, LinksOperation<UInt128>.ParseName(Run(new LinksOperation<UInt128>.GetName(1))));
        Assert.Equal(UInt128.One, LinksOperation<UInt128>.ParseLinkReply(Run(new LinksOperation<UInt128>.GetByName(name))));
        Assert.Empty(Run(new LinksOperation<UInt128>.RemoveName(1)));
        Assert.Empty(Run(new LinksOperation<UInt128>.GetName(1)));
        Assert.Null(LinksServer.ErrorMessage(Run(new LinksOperation<UInt128>.Update(1, 0, ulong.MaxValue))));
        Assert.Null(LinksServer.ErrorMessage(Run(new LinksOperation<UInt128>.Delete(1))));
        Assert.Equal(persisted, Bytes(store));
        Assert.Equal(2UL, tx.CreateLink(0, 0));
        tx.Prepare(Token(100));
        Assert.NotNull(LinksServer.ErrorMessage(Run(new LinksOperation<UInt128>.Count(LinksRestriction<UInt128>.All))));
    }

    [Fact]
    public void DirectDocumentsAreAdmittedBeforeRecursiveFormattingAndBeforeEdits()
    {
        using var tx = Session(new Store());
        var nested = LinoFormat.Reference("leaf");
        for (var i = 0; i < 66; i++) nested = LinoFormat.Link(null, [nested]);
        IReadOnlyList<LinoLink>[] invalid = [[nested], Enumerable.Repeat(LinoFormat.Reference("x"), 4097).ToArray(),
            LinksServer.QueryDocument("#" + new string('x', 1024 * 1024)),
            LinoFormat.ParseDocument("(query: (nested: invalid))")];
        using var traced = LinksServer.Bind("127.0.0.1", 0, new() { Trace = true });
        foreach (var request in invalid)
        {
            Assert.NotNull(LinksServer.ErrorMessage(LinksServer.ExecuteRequest(tx.QueryLinks, request)));
            Assert.NotNull(LinksServer.ErrorMessage(traced.Execute(tx.QueryLinks, request)));
        }
        Assert.Empty(tx.Rows());
        Assert.Equal(1UL, tx.CreateLink(0, 0));
    }

    [Fact]
    public void LosslessTcpAndGenericRemoteKeepPrivateWritesAndOwnedWideCallbacks()
    {
        foreach (var protocol in RunningServer.Protocols())
        {
            var store = new Store();
            Batch(store).Write(Token(10), 0, [new(ulong.MaxValue, [], new(0, ulong.MaxValue, null))]);
            var persisted = Bytes(store);
            using var tx = Session(store);
            using var server = new CausalServer(tx.QueryLinks);
            using var client = LinksClient.Connect("127.0.0.1", server.Port, protocol);
            client.QueryLossless("() ((1: 18446744073709551615 1))");
            foreach (var name in new[] { "4294967312", "18446744073709551615", "*", "$x", "", "'\"雪\0\n" })
            {
                client.Request(new LinksOperation<UInt128>.SetName(1, name).ToDocument());
                var literal = ReferenceLiteral.Encode(name);
                client.QueryLossless($"# ignored )\n(({literal}: $s $t)) (({literal}: $t $s))");
                Assert.Equal(name, tx.GetName(1));
            }
            using var remote = new RemoteLinks<UInt128>(LinksClient.Connect("127.0.0.1", server.Port, protocol));
            IList<UInt128>? observed = null;
            remote.Each([ulong.MaxValue], row => { observed = row; return remote.Constants.Break; });
            Assert.Equal(new UInt128[] { ulong.MaxValue, 0, ulong.MaxValue }, observed!);
            remote.Update([ulong.MaxValue], [ulong.MaxValue, ulong.MaxValue, 0], null);
            Assert.Equal(new UInt128[] { ulong.MaxValue, 0, ulong.MaxValue }, observed!);
            Assert.Equal(persisted, Bytes(store));
            tx.Prepare(Token(100));
            Assert.Throws<LinoProtocolException>(() => client.QueryLossless(""));
            Assert.Throws<LinoProtocolException>(() => remote.Count(null));
        }
    }

    [Fact]
    public void CheckedTextAdmissionRaisesProtocolErrorsBeforeCanonicalization()
    {
        var limits = ProtocolLimits.Default.ForQuery(new QueryLimits());
        foreach (var text in new[] { new string('(', 65) + new string(')', 65),
            string.Join(' ', Enumerable.Repeat("x", 4097)), "(" })
        {
            using var wire = new MemoryStream();
            TextLinoProtocol.WriteText(wire, text); wire.Position = 0;
            Assert.Throws<LinoProtocolException>(() => LinoProtocols.ReadAnyDocument(new PacketReader(wire), limits));
        }
    }

    [Fact]
    public void MalformedOrOversizedWireRequestsCloseCleanlyBeforePrivateEdits()
    {
        foreach (var protocol in RunningServer.Protocols())
        {
            using var tx = Session(new Store());
            using var server = new CausalServer(tx.QueryLinks);
            var nested = LinoFormat.Reference("leaf");
            for (var i = 0; i < 66; i++) nested = LinoFormat.Link(null, [nested]);
            IReadOnlyList<LinoLink>[] documents = [[nested], Enumerable.Repeat(LinoFormat.Reference("x"), 4097).ToArray(),
                LinksServer.QueryDocument(new string('x', 1024 * 1024 + 1))];
            var writer = protocol is BinaryLinoProtocol binary ? binary with { Limits = DecodeLimits.Unlimited } : protocol;
            foreach (var document in documents)
            {
                using var peer = new TcpClient("127.0.0.1", server.Port) { SendTimeout = 30000, ReceiveTimeout = 30000 };
                using var stream = peer.GetStream();
                using var encoded = new MemoryStream();
                writer.WriteDocument(encoded, document);
                var bytes = encoded.ToArray();
                // Oversized binary declarations must reject before reading the payload.
                // Send only a finite prefix so refusal cannot depend on buffering it all.
                var prefix = protocol is BinaryLinoProtocol && bytes.Length > 4096;
                stream.Write(bytes.AsSpan(0, prefix ? 4096 : bytes.Length));
                peer.Client.Shutdown(SocketShutdown.Send);
                var reply = LinoProtocols.ReadAnyDocument(new PacketReader(stream));
                var error = LinksServer.ErrorMessage(reply!.Value.Document);
                Assert.NotNull(error);
                if (prefix) Assert.Contains("limit", error, StringComparison.OrdinalIgnoreCase);
            }
            if (protocol is TextLinoProtocol)
                foreach (var text in new[] { "#" + new string('x', 1024 * 1024),
                    new string('(', 65) + new string(')', 65), "(" })
                {
                    using var peer = new TcpClient("127.0.0.1", server.Port) { SendTimeout = 30000, ReceiveTimeout = 30000 };
                    using var stream = peer.GetStream();
                    TextLinoProtocol.WriteText(stream, text);
                    Assert.NotNull(LinksServer.ErrorMessage(LinoProtocols.ReadAnyDocument(new PacketReader(stream))!.Value.Document));
                }
            using var client = LinksClient.Connect("127.0.0.1", server.Port, protocol);
            Assert.Empty(client.QueryLossless("# admitted comment"));
            Assert.Empty(tx.Rows());
            Assert.Equal(1UL, tx.CreateLink(0, 0));
        }
    }

    [Fact]
    public void ExpandedNamedReplyIsRefusedBeforeTransportedQueryEdits()
    {
        using var tx = Session(new Store(), 1, 1000);
        for (var i = 0; i < 64; i++)
        {
            var id = tx.CreateLink(0, 0);
            tx.UpdateLink(id, id, id == 1 ? 1 : id - 1);
            tx.SetName(id, id.ToString("D4") + new string('n', 4092));
        }
        var before = tx.Rows();
        foreach (var protocol in RunningServer.Protocols())
        {
            var reply = LinksServer.ExecuteRequest(tx.QueryLinks, RoundTrip(protocol,
                LinksServer.QueryDocument("(($i: $s $t)) (($i: $t $s))")));
            Assert.Contains("reply", LinksServer.ErrorMessage(reply)!, StringComparison.OrdinalIgnoreCase);
            Assert.Equal(before, tx.Rows());
        }
        Assert.Equal(65UL, tx.CreateLink(0, 0));
    }

    private sealed class CausalServer : IDisposable
    {
        private readonly LinksServer _server;
        private readonly Thread _thread;
        private Exception? _failure;
        internal int Port => _server.LocalEndPoint.Port;
        internal CausalServer(INamedTypesLinks<UInt128> links, LinksServerOptions? options = null)
        {
            _server = LinksServer.Bind("127.0.0.1", 0, options);
            _thread = new Thread(() => { try { _server.Serve(links); } catch (Exception error) { _failure = error; } }) { IsBackground = true };
            _thread.Start();
        }
        public void Dispose()
        {
            _server.Shutdown();
            Assert.True(_thread.Join(TimeSpan.FromSeconds(30)));
            Assert.Null(_failure);
        }
    }
}
