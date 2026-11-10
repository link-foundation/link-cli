using System.Numerics;
using Foundation.Data.Doublets.Cli.Engine;
using Foundation.Data.Doublets.Cli.Protocol;
using Link.Foundation.Links.Notation.Binary;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

public sealed class GenericLinksOperationTests
{
    [Fact]
    public void EveryOperationAndCodecUsesTheSelectedUnsignedAddressRange()
    {
        Exercise((ushort)(ushort.MaxValue - 1));
        Exercise(uint.MaxValue - 1);
        Exercise(ulong.MaxValue - 1);
        Exercise(UInt128.MaxValue - 1);
        Exercise<UInt128>(ulong.MaxValue);
    }

    [Fact]
    public void ConcreteRestrictionsStayDistinctFromTheStoreWildcard()
    {
        Exercise((ushort)(ushort.MaxValue - 3));
        Exercise(uint.MaxValue - 3);
        Exercise(ulong.MaxValue - 3);
        Exercise(UInt128.MaxValue - 3);
    }

    [Fact]
    public void NumericMaximumStaysConcreteThroughOperationAndReplyDispatch()
    {
        Maximum<ushort>(); Maximum<uint>(); Maximum<ulong>(); Maximum<UInt128>();
    }

    private static void Maximum<T>()
        where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
    {
        foreach (var protocol in RunningServer.Protocols())
        {
            var engine = new LinksEngine<T>(new MemoryCommitLog());
            using var tx = engine.Begin();
            var operation = new LinksOperation<T>.Create(T.MaxValue, T.MaxValue);
            using var request = new MemoryStream();
            protocol.WriteDocument(request, operation.ToDocument()); request.Position = 0;
            var document = protocol.ReadDocument(new PacketReader(request))!;
            Assert.Equal(operation, LinksOperation<T>.FromDocument(document));
            var reply = LinksServer.ExecuteRequest(tx, document);
            Assert.Null(LinksServer.ErrorMessage(reply));
            using var encoded = new MemoryStream();
            protocol.WriteDocument(encoded, reply); encoded.Position = 0;
            var row = Assert.Single(LinksOperation<T>.ParseChanges(protocol.ReadDocument(new PacketReader(encoded))!)).After;
            Assert.Equal(T.MaxValue, row.Source); Assert.Equal(T.MaxValue, row.Target);
            Assert.Empty(engine.Snapshot().Rows);
        }
    }

    private static void Exercise<T>(T high)
        where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
    {
        foreach (var protocol in RunningServer.Protocols())
        {
            var engine = new LinksEngine<T>(new MemoryCommitLog());
            using var tx = engine.Begin();
            IReadOnlyList<LinoLink> Run(LinksOperation<T> operation)
            {
                using var request = new MemoryStream();
                protocol.WriteDocument(request, operation.ToDocument()); request.Position = 0;
                var document = protocol.ReadDocument(new PacketReader(request))!;
                Assert.Equal(operation, LinksOperation<T>.FromDocument(document));
                var result = LinksServer.ExecuteRequest(tx, document);
                Assert.Null(LinksServer.ErrorMessage(result));
                using var reply = new MemoryStream();
                protocol.WriteDocument(reply, result); reply.Position = 0;
                return protocol.ReadDocument(new PacketReader(reply))!;
            }
            Assert.Equal(T.One, Assert.Single(LinksOperation<T>.ParseChanges(Run(new LinksOperation<T>.Create(T.Zero, high)))).After.Index);
            Assert.Equal(high, tx.GetLink(T.One)!.Value.Target);
            tx.EnsureCreated(high);
            tx.UpdateLink(high, high, T.Zero);
            Assert.Equal(T.One, LinksOperation<T>.ParseCount(Run(new LinksOperation<T>.Count(LinksRestriction<T>.Of(high)))));
            Assert.Equal(T.CreateChecked(2), LinksOperation<T>.ParseCount(Run(new LinksOperation<T>.Count(LinksRestriction<T>.All))));
            Assert.Equal(new[] { T.One, high }, LinksOperation<T>.ParseLinks(Run(new LinksOperation<T>.Each(LinksRestriction<T>.All))).Select(row => row.Index));
            const string name = "numeric 18446744073709551615\0雪\n";
            Assert.Equal(high, LinksOperation<T>.ParseLinkReply(Run(new LinksOperation<T>.SetName(high, name))));
            Assert.Equal(name, LinksOperation<T>.ParseName(Run(new LinksOperation<T>.GetName(high))));
            Assert.Equal(high, LinksOperation<T>.ParseLinkReply(Run(new LinksOperation<T>.GetByName(name))));
            Assert.Empty(Run(new LinksOperation<T>.RemoveName(high)));
            Assert.Null(LinksOperation<T>.ParseName(Run(new LinksOperation<T>.GetName(high))));
            var updated = Assert.Single(LinksOperation<T>.ParseChanges(Run(new LinksOperation<T>.Update(high, T.Zero, high))));
            Assert.Equal(high, updated.Before.Source); Assert.Equal(high, updated.After.Target);
            Assert.Equal(high, Assert.Single(LinksOperation<T>.ParseChanges(Run(new LinksOperation<T>.Delete(high)))).Before.Index);
            Assert.Empty(engine.Snapshot().Rows);
        }
    }
}
