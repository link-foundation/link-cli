using Foundation.Data.Doublets.Cli.Protocol;
using Platform.Data;
using Platform.Data.Doublets;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

/// <summary>
/// A whole store survives <c>--export-binary</c> and <c>--import-binary</c>: links keep their addresses, holes stay
/// holes, names come back, and the archive bytes match the Rust port.
/// </summary>
public sealed class StoreArchiveTests
{
    /// <summary>
    /// The archive of <see cref="Fill"/>, byte for byte; the Rust <c>store_archive_tests</c> assert the same bytes.
    /// </summary>
    /// <remarks>
    /// Links packet, 13 bytes: <c>12</c> explicit layout, <c>02</c> sections; <c>20 01</c> one doublet of 1-byte
    /// references, <c>01 01</c>; <c>24 01 02</c> a section after a gap of one address (the hole at 2) with two
    /// doublets, <c>03 03 01 03</c>. Names packet, 17 bytes: <c>13</c> explicit layout with external references,
    /// <c>02</c> sections; <c>21 02</c> two links of two 2-byte references, <c>ff ff ff 9f</c> (1, <c>a</c>) and
    /// <c>ff fd ff 17</c> (3, <c>é</c>); <c>30 01</c> one link of three 1-byte references, <c>fc 9f 9e</c>
    /// (4, <c>a</c>, <c>b</c>).
    /// </remarks>
    private const string Archive =
        "12 02 20 01 24 01 02 01 01 03 03 01 03 13 02 21 02 30 01 ff ff 9f ff fd ff 17 ff fc 9f 9e";

    private sealed class LocalStore : IDisposable
    {
        private readonly string _databaseFilename = Path.GetTempFileName();

        public LocalStore() => Links = new NamedTypesDecorator<uint>(_databaseFilename);

        public NamedTypesDecorator<uint> Links { get; }

        public void Dispose()
        {
            Links.Dispose();
            File.Delete(_databaseFilename);
            File.Delete(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(_databaseFilename));
        }
    }

    /// <summary>Links <c>1: 1 1</c> named <c>a</c>, <c>3: 3 3</c> named <c>é</c> and <c>4: 1 3</c> named <c>ab</c>, with a hole at 2.</summary>
    private static void Fill(INamedTypesLinks<uint> links)
    {
        for (var i = 0; i < 3; i++)
        {
            var id = links.Create();
            links.Update(id, id, id);
        }
        var last = links.Create();
        Assert.Equal(4u, last);
        links.Update(last, 1u, 3u);
        links.Delete(2u);
        links.SetName(1, "a");
        links.SetName(3, "é");
        links.SetName(4, "ab");
    }

    private static byte[] Export(INamedTypesLinks<uint> links)
    {
        using var output = new MemoryStream();
        StoreArchive.Export(links, output);
        return output.ToArray();
    }

    private static void Import(INamedTypesLinks<uint> links, byte[] archive) =>
        StoreArchive.Import(links, new MemoryStream(archive));

    private static string Hex(byte[] bytes) => string.Join(" ", bytes.Select(value => value.ToString("x2")));

    private static byte[] Bytes(string hex) =>
        hex.Split(' ', StringSplitOptions.RemoveEmptyEntries).Select(value => Convert.ToByte(value, 16)).ToArray();

    private static List<DoubletLink> AllLinks(INamedTypesLinks<uint> links) =>
        links.All(new DoubletLink(links.Constants.Any, links.Constants.Any, links.Constants.Any))
            .Select(link => new DoubletLink(link))
            .OrderBy(link => link.Index)
            .ToList();

    private static List<string?> Names(INamedTypesLinks<uint> links) =>
        Enumerable.Range(1, 4).Select(id => links.GetName((uint)id)).ToList();

    private static byte[] Packet(bool externalReferences, params (ulong Address, PacketReference[] References)[] links) =>
        LinksPacket.Pack(externalReferences, links, true).ToBytes();

    private static (ulong, PacketReference[]) Doublet(ulong address, ulong source, ulong target) =>
        (address, new[] { PacketReference.Internal(source), PacketReference.Internal(target) });

    private static string ImportError(byte[] archive)
    {
        using var store = new LocalStore();
        return Assert.Throws<LinoProtocolException>(() => Import(store.Links, archive)).Message;
    }

    [Fact]
    public void AStoreExportsToTheGoldenArchive()
    {
        using var source = new LocalStore();
        Fill(source.Links);

        Assert.Equal(Archive, Hex(Export(source.Links)));
    }

    [Fact]
    public void AnImportedArchiveRestoresLinksHolesAndNames()
    {
        using var source = new LocalStore();
        Fill(source.Links);
        using var target = new LocalStore();

        Import(target.Links, Bytes(Archive));

        Assert.Equal(AllLinks(source.Links), AllLinks(target.Links));
        Assert.False(target.Links.Exists(2u), "the hole at 2 stays a hole");
        Assert.Equal(Names(source.Links), Names(target.Links));
        Assert.Equal(Archive, Hex(Export(target.Links)));
    }

    [Fact]
    public void AnEmptyStoreIsTwoEmptyPackets()
    {
        using var empty = new LocalStore();
        Assert.Equal("10 00 11 00", Hex(Export(empty.Links)));

        Import(empty.Links, Bytes("10 00 11 00"));
        Assert.Empty(AllLinks(empty.Links));
    }

    [Fact]
    public void AnArchiveRoundTripsThroughARemoteStore()
    {
        using var server = new RunningServer();
        using var remote = server.Remote(new TextLinoProtocol());

        Import(remote, Bytes(Archive));

        Assert.Equal(Archive, Hex(Export(remote)));
        Assert.False(remote.Exists(2u));
    }

    [Fact]
    public void AnArchiveImportsIntoAStoreThatAlreadyHasTheLinks()
    {
        using var target = new LocalStore();
        Fill(target.Links);

        Import(target.Links, Bytes(Archive));

        Assert.Equal(Archive, Hex(Export(target.Links)));
    }

    [Fact]
    public void ATruncatedArchiveIsRejected()
    {
        Assert.Contains("ends before its links", ImportError(Array.Empty<byte>()));
        Assert.Contains("ends before its names", ImportError(Bytes("10 00")));
        Assert.Contains("malformed", ImportError(Bytes("10 00 11")));
    }

    [Fact]
    public void TrailingBytesAreRejected() =>
        Assert.Contains("trailing bytes", ImportError(Bytes("10 00 11 00 10")));

    [Fact]
    public void ALinkMustBeADoubletOfLinkAddresses()
    {
        var triplet = (1UL, new[] { PacketReference.Internal(1), PacketReference.Internal(1), PacketReference.Internal(1) });
        var external = (1UL, new[] { PacketReference.External(1), PacketReference.Internal(1) });
        foreach (var link in new[] { triplet, external })
        {
            var archive = Packet(true, link).Concat(Packet(true)).ToArray();
            Assert.Contains("is not a doublet of link addresses", ImportError(archive));
        }
    }

    [Fact]
    public void AddressesMustFitA32BitStore()
    {
        var tooLarge = (ulong)uint.MaxValue + 1;
        foreach (var link in new[] { Doublet(tooLarge, 1, 1), Doublet(1, tooLarge, 1) })
        {
            var archive = Packet(false, link).Concat(Packet(true)).ToArray();
            Assert.Contains("does not fit a 32-bit store", ImportError(archive));
        }
    }

    [Theory]
    [InlineData(false, 0UL, "only external values")]
    [InlineData(true, 0xD800UL, "invalid code point 55296")]
    [InlineData(true, 1UL << 40, "invalid code point")]
    [InlineData(true, null, "does not fit a 32-bit store")]
    public void NamesMustBeAnAddressAndCodePoints(bool external, ulong? codePoint, string message)
    {
        var name = !external
            ? new[] { PacketReference.Internal(1) }
            : codePoint is { } value
                ? new[] { PacketReference.External(1), PacketReference.External(value) }
                : new[] { PacketReference.External(1UL << 40) };
        var archive = Packet(false, Doublet(1, 1, 1)).Concat(Packet(true, (1UL, name))).ToArray();

        Assert.Contains(message, ImportError(archive));
    }

    [Fact]
    public void AnEmptyNameIsKept()
    {
        var archive = Packet(false, Doublet(1, 1, 1)).Concat(Packet(true, (1UL, new[] { PacketReference.External(1) }))).ToArray();
        using var store = new LocalStore();

        Import(store.Links, archive);

        Assert.Equal("", store.Links.GetName(1));
        Assert.Equal(Hex(archive), Hex(Export(store.Links)));
    }
}
