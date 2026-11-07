using Foundation.Data.Doublets.Cli.Tests.Protocol;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Memory;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>The refusals and rarely taken paths of the small library types.</summary>
public class LibraryEdgeTests
{
    [Fact]
    public void AnOffsetPastTheLastAddressIsRefused()
    {
        var map = new OffsetAddressMap<byte>(200);
        Assert.Equal((byte)250, map.ToOuter(50));
        Assert.Throws<OverflowException>(() => map.ToOuter(100));
    }

    [Fact]
    public void AnAddressTableRefusesTheNullLinkAndSkipsAddressesItHolds()
    {
        Assert.Throws<ArgumentException>(() => new AddressTable<uint>(0));
        var table = new AddressTable<uint>(1);
        Assert.Throws<ArgumentException>(() => table.Add(0, 1));
        Assert.Throws<ArgumentException>(() => table.Add(1, 0));
        table.Add(1, 10);
        Assert.True(table.TryToInner(1, out var inner));
        Assert.Equal(10u, inner);
        Assert.False(table.TryToInner(2, out _));
        Assert.Equal(2u, table.ToOuter(20));
    }

    [Fact]
    public void AcquiringALockCreatesItsDirectory()
    {
        var directory = Path.Combine(Path.GetTempPath(), Guid.NewGuid().ToString("N"));
        try
        {
            using var held = LinksFileLock.TryAcquire(Path.Combine(directory, "db.links.lock"), LockMode.Exclusive);
            Assert.NotNull(held);
            Assert.True(Directory.Exists(directory));
        }
        finally
        {
            Directory.Delete(directory, true);
        }
    }

    [Fact]
    public void AStorageRevisionNamesItsParts()
    {
        var revision = new StorageRevision(3, 4);
        Assert.Equal("StorageRevision(Length=3, ModifiedTicks=4)", revision.ToString());
        Assert.Equal(new StorageRevision(3, 4).GetHashCode(), revision.GetHashCode());
        Assert.Equal(new StorageRevision(5, 4), revision with { Length = 5 });
        var (length, ticks) = revision;
        Assert.Equal((3L, 4L), (length, ticks));
    }

    [Fact]
    public void StringStorageTypesAreFoundAgainAndListed()
    {
        var path = Path.GetTempFileName();
        try
        {
            var constants = new LinksConstants<uint>(enableExternalReferencesSupport: true);
            using var memory = new FileMappedResizableDirectMemory(path, UnitedMemoryLinks<uint>.DefaultLinksSizeStep);
            using var store = new UnitedMemoryLinks<uint>(memory, UnitedMemoryLinks<uint>.DefaultLinksSizeStep, constants, Platform.Data.Doublets.Memory.IndexTreeType.Default);
            var links = store.DecorateWithAutomaticUniquenessAndUsagesRepointing();
            var storage = new UnicodeStringStorage<uint>(links);
            var type = storage.GetOrCreateType("Marker");
            Assert.Equal(type, storage.GetOrCreateType("Marker"));
            Assert.True(storage.IsType(type));
            Assert.False(storage.IsType(storage.CreateString("text")));
            Assert.Contains(storage.GetTypes(), link => links.GetIndex(link!) == type);
        }
        finally
        {
            File.Delete(path);
        }
    }

    [Fact]
    public void ImportingToAnAbsentIndexCreatesIt()
    {
        var path = Path.GetTempFileName();
        try
        {
            using var links = new NamedTypesDecorator<uint>(path, false);
            LinoDatabaseInput.UpdateLink(links, 3, 1, 2);
            var row = new Link<uint>(links.GetLink(3u));
            Assert.Equal((1u, 2u), (row.Source, row.Target));
        }
        finally
        {
            File.Delete(path);
            File.Delete(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(path));
        }
    }

    [Fact]
    public void ATokenNeedsASubjectAndAForeignLinkOfItsNameAuthenticatesNobody()
    {
        PermissionsDecoratorTests.WithWorld(w =>
        {
            var table = new LinksTokenTable(w.Links);
            Assert.Throws<ArgumentException>(() => table.Grant("token", w.Links.Constants.Null));
            table.Grant("other", w.V["alice"]);
            var foreign = w.Links.CreateAndUpdate(0u, 0u);
            w.Links.Update(foreign, w.V["alice"], foreign);
            w.Links.SetName(foreign, LinksTokenTable.NameOf("forged"));
            Assert.Null(table.Authenticate("forged"));
        });
    }

    [Theory]
    [InlineData("read", PermissionAccess.Read)]
    [InlineData("write", PermissionAccess.Write)]
    [InlineData("reference", PermissionAccess.Reference)]
    public void AccessNamesParse(string name, PermissionAccess access)
    {
        Assert.Equal(access, PermissionPolicy<uint>.ParseAccess(name));
    }

    [Fact]
    public void AnUnknownAccessIsRefusedAndAPolicyExposesItsStores()
    {
        Assert.Throws<ArgumentException>(() => PermissionPolicy<uint>.ParseAccess("own"));
        PermissionsDecoratorTests.WithWorld(w =>
        {
            Assert.NotNull(w.Policy.Rules);
            Assert.NotNull(w.Policy.Types);
        });
    }

    [Theory]
    [InlineData("")]
    [InlineData("single")]
    public void ATriggerQueryNeedsBothHalves(string query)
    {
        Assert.Throws<ArgumentException>(() => PersistentTransformationDecorator.PersistentTransformationQuery.Parse(query));
    }

    [Fact]
    public void ATriggerQueryMayGiveItsHalvesAsTwoLinks()
    {
        var query = PersistentTransformationDecorator.PersistentTransformationQuery.Parse("(a)\n(c d)");
        Assert.Contains('a', query.Condition);
        Assert.Contains('c', query.Substitution);
    }

    [Fact]
    public void AUnionPlacesAnUnclaimedAddressInItsPrimaryAndHasNoNameForAnAbsentLink()
    {
        using var stores = new UnionAndRemapLinksTests.Stores();
        var local = stores.New();
        var union = new UnionLinksDecorator<uint>(local, stores.New());
        union.EnsureCreated(7u);
        Assert.True(local.Exists(7u));
        Assert.Null(union.GetName(99u));
    }

    [Fact]
    public void ACallerOwnedStoreHasNoFlushOfItsOwn()
    {
        using var stores = new UnionAndRemapLinksTests.Stores();
        var links = stores.New();
        Assert.False(links.SupportsDurableFlush);
        Assert.Throws<InvalidOperationException>(links.Flush);
        var untyped = (System.Collections.IEnumerable)links;
        Assert.NotNull(untyped.GetEnumerator());
    }

    [Fact]
    public void NamedLinksCanWrapACallerOwnedStore()
    {
        using var stores = new UnionAndRemapLinksTests.Stores();
        var names = Path.GetTempFileName();
        try
        {
            var links = new NamedLinksDecorator<uint>(stores.New(), names);
            var link = links.CreateAndUpdate(0u, 0u);
            links.SetName(link, "kept");
            Assert.Equal(link, links.GetByName("kept"));
            links.Dispose();
        }
        finally
        {
            File.Delete(names);
        }
    }
}
