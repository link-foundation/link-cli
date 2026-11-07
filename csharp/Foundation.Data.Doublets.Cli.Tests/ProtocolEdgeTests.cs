using Foundation.Data.Doublets.Cli.Protocol;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>The value semantics and refusals of the protocol types.</summary>
public sealed class ProtocolEdgeTests
{
    [Fact]
    public void EqualRestrictionsHashAlike()
    {
        Assert.Equal(LinksRestriction<uint>.Of(1u, null).GetHashCode(), LinksRestriction<uint>.Of(1u, null).GetHashCode());
        Assert.NotEqual(LinksRestriction<uint>.Of(1u), LinksRestriction<uint>.Of(2u));
    }

    [Fact]
    public void EqualNotificationsHashAlike()
    {
        var change = (new DoubletLink(1, 2, 3), new DoubletLink(1, 2, 4));
        var left = new LinksNotification(7, [change]);
        var right = new LinksNotification(7, [change]);

        Assert.Equal(left, right);
        Assert.Equal(left.GetHashCode(), right.GetHashCode());
        Assert.NotEqual(left, new LinksNotification(8, [change]));
    }

    [Fact]
    public void EqualTripletNotificationsHashAlike()
    {
        var left = new TripletsNotification(7, []);
        var right = new TripletsNotification(7, []);

        Assert.Equal(left, right);
        Assert.Equal(left.GetHashCode(), right.GetHashCode());
    }

    [Fact]
    public void ATripletOfTheWrongShapeIsMalformed()
    {
        var refused = Assert.Throws<LinoProtocolException>(() =>
            TripletsOperation.ParseTriplet(new LinoLink("only", new List<LinoLink>())));

        Assert.Equal(LinoProtocolErrorKind.Malformed, refused.Kind);
    }

    [Fact]
    public void AStreamWithoutTimeoutsAnswersOnceAByteArrives()
    {
        using var wire = new MemoryStream([(byte)'(']);
        using var connection = new LinoConnection(wire, new TextLinoProtocol());

        Assert.False(wire.CanTimeout);
        Assert.True(connection.WaitForMessage(TimeSpan.FromSeconds(1)));
    }
}
