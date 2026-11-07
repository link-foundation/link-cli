// What the ordinary paths through the query processors and the decorators do not meet.
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class LibraryRareEdgeTests
{
    private static readonly Guid Tx = Guid.Parse("00000000-0000-0000-0000-000000000007");

    private static JournalRecord<uint> Record(TransitionKind kind, Link<uint> before, Link<uint> after) =>
        new(new Transition<uint>(Tx, 1, DateTimeOffset.UnixEpoch, kind, before, after));

    private static string Marker(params JournalRecord<uint>[] undo) =>
        VersionControlDecorator.EncodeMovement(new("main", 1, "main", 0, undo, []));

    public static TheoryData<TransitionKind, uint, uint, bool> MovementRecords => new()
    {
        { TransitionKind.Create, 0, 5, true },
        { TransitionKind.Create, 5, 5, false },
        { TransitionKind.Delete, 5, 0, true },
        { TransitionKind.Delete, 0, 0, false },
        { TransitionKind.Update, 5, 5, true },
        { TransitionKind.Update, 5, 6, false },
        { (TransitionKind)99, 5, 5, false },
    };

    [Theory]
    [MemberData(nameof(MovementRecords))]
    public void AMovementKeepsOnlyRecordsOfTheirKind(TransitionKind kind, uint before, uint after, bool kept)
    {
        var marker = Marker(Record(kind, new Link<uint>(before, 1, 1), new Link<uint>(after, 1, 1)));
        Assert.Equal(kept, VersionControlDecorator.TryDecodeMovement(marker, out var intent));
        if (kept) Assert.Equal(kind, intent!.Undo[0].Kind);
    }

    [Fact]
    public void AnImportReferenceNeedsAValue()
    {
        const string text = "(a: \"\" 1)";
        var directory = Directory.CreateTempSubdirectory("rare-edge-").FullName;
        try
        {
            using var named = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            Assert.Throws<FormatException>(() => LinoDatabaseInput.ImportText(named, text));
        }
        finally { Directory.Delete(directory, true); }
    }

    [Theory]
    [InlineData("foo")]
    [InlineData("(a b)")]
    public void ABasicQueryWithoutRestrictionAndSubstitutionWritesNothing(string query)
    {
        var directory = Directory.CreateTempSubdirectory("rare-edge-").FullName;
        try
        {
            using var named = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            BasicQueryProcessor.ProcessQuery(named, query);
            Assert.Equal(0u, named.Count(named.Constants.Any, named.Constants.Any, named.Constants.Any));
        }
        finally { Directory.Delete(directory, true); }
    }

    [Fact]
    public void AnUpdateThatMergesIntoAnotherLinkDropsItsName()
    {
        var directory = Directory.CreateTempSubdirectory("rare-edge-").FullName;
        try
        {
            using var named = new NamedLinksDecorator<uint>(Path.Combine(directory, "data.links"), false);
            var kept = named.GetOrCreate(1u, 2u);
            var merged = named.GetOrCreate(3u, 4u);
            named.SetName(merged, "gone");
            named.Update(new Link<uint>(merged, named.Constants.Any, named.Constants.Any), new Link<uint>(merged, 1u, 2u), (_, _) => named.Constants.Continue);
            Assert.NotEqual(kept, merged);
            Assert.False(named.Exists(merged));
            Assert.Null(named.GetName(merged));
        }
        finally { Directory.Delete(directory, true); }
    }

    [Fact]
    public void ANameOnlyOneMemberGaveAnAbsentLinkIsStillRead()
    {
        using var stores = new UnionAndRemapLinksTests.Stores();
        var local = stores.New();
        var server = stores.New();
        server.SetName(99u, "ghost");
        var union = new UnionLinksDecorator<uint>(local, server);
        Assert.Equal("ghost", union.GetName(99u));
    }

    /// <summary>Lets everything be written and everything but one link be read.</summary>
    private sealed class HidesOneLink(uint hidden) : IPermissionPolicy<uint>
    {
        public bool CanRead(uint subject, IList<uint> link) => link[0] != hidden;
        public bool CanWrite(uint subject, IList<uint> link) => true;
        public PermissionMarkers<uint> Markers => default;
        public bool CanCreate(uint subject) => true;
    }

    [Fact]
    public void AWritableTypeTheSubjectCannotReferIsNotFound()
    {
        PermissionsDecoratorTests.WithWorld(w =>
        {
            var triplets = new TripletsOverDoublets<uint>(w.Links);
            var view = new TripletsPermissionsDecorator<uint>(triplets, new HidesOneLink(w.T["Secret"]), () => w.V["alice"]);
            var refused = Assert.Throws<ArgumentException>(() => view.CreateTriplet(w.T["Secret"], 0u, 0u));
            Assert.StartsWith("Link not found", refused.Message, StringComparison.Ordinal);
            var own = view.CreateTriplet(w.T["Note"], 0u, 0u);
            refused = Assert.Throws<ArgumentException>(() => view.UpdateTriplet(own, w.T["Secret"], 0u, 0u));
            Assert.StartsWith("Link not found", refused.Message, StringComparison.Ordinal);
        });
    }

    [Theory]
    [InlineData("(1: 2 3 4)\n()")]
    [InlineData("((1: knows 0 0) ())")]
    public void TripletQueriesReadBareAndNamedPatterns(string query)
    {
        PermissionsDecoratorTests.WithWorld(w =>
            Assert.Empty(TripletsQueryProcessor.ProcessQuery(new TripletsOverDoublets<uint>(w.Links), query)));
    }

    [Fact]
    public void AReferenceThatIsNothingElseIsAName() =>
        Assert.Equal(TripletsQueryProcessor.ReferenceKind.Name, TripletsQueryProcessor.Reference.Parse("plain").Kind);
}
