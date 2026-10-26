using Foundation.Data.Doublets.Cli.Protocol;
using Link.Foundation.Links.Notation.Binary;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests;

public class FallibleCompositionTests
{
    private sealed class AllocationFault(string path) : NamedTypesDecorator<uint>(path)
    {
        public IOException Error { get; } = new("injected allocation");
        public bool Fail { get; set; }
        public override uint Create(IList<uint>? substitution, WriteHandler<uint>? handler)
        {
            if (Fail) throw Error;
            return base.Create(substitution, handler);
        }
    }

    private static (uint, uint, uint)[] Rows(INamedTypesLinks<uint> links) =>
        links.All(new DoubletLink(links.Constants.Any, links.Constants.Any, links.Constants.Any))
            .Select(row => new DoubletLink(row)).OrderBy(row => row.Index)
            .Select(row => (row.Index, row.Source, row.Target)).ToArray();

    private static void Import(INamedTypesLinks<uint> links, string operation)
    {
        if (operation != "import-archive")
        {
            LinoDatabaseInput.ImportText(links, operation == "import-explicit"
                ? "(8: 1 8)" : "('imported 雪': 1 0)");
            return;
        }
        var rows = LinksPacket.Pack(false, new[]
        {
            (8UL, new[] { PacketReference.Internal(1), PacketReference.Internal(8) })
        }, true).ToBytes();
        var name = new[] { PacketReference.External(8) }
            .Concat("imported 雪".Select(c => PacketReference.External(c))).ToArray();
        var names = LinksPacket.Pack(true, new[] { (1UL, name) }, true).ToBytes();
        using var input = new MemoryStream(rows.Concat(names).ToArray());
        StoreArchive.Import(links, input);
    }

    [Theory]
    [InlineData("create")]
    [InlineData("ensure")]
    [InlineData("planner")]
    [InlineData("query")]
    [InlineData("schema")]
    [InlineData("import-explicit")]
    [InlineData("import-named")]
    [InlineData("import-archive")]
    public void AllocationExceptionSurvivesTriggerCompositionAndFollowingReopening(string operation)
    {
        // C# uses exceptions throughout its write API. This is a passing
        // control for the separately reproduced Rust Result/bridge defect.
        var directory = Directory.CreateTempSubdirectory("fallible-composition-").FullName;
        var path = Path.Combine(directory, "data.links");
        (uint, uint, uint)[] expected;
        uint following;
        try
        {
            using (var data = new AllocationFault(path))
            {
                var links = new PersistentTransformationDecorator(data, data);
                if (operation.StartsWith("import-", StringComparison.Ordinal))
                {
                    Assert.Equal(1u, data.CreateAndUpdate(1u, 1u));
                    data.SetName(1, "seed 雪");
                }
                if (operation == "schema")
                    foreach (var name in new[] { "Type", "Trigger", "Once", "Always", "Condition", "Substitution" })
                    {
                        var id = data.CreateAndUpdate(0u, 0u);
                        data.SetName(id, name); data.Update(id, id, id);
                    }
                var before = Rows(links);
                data.Fail = true;
                var error = Assert.Throws<IOException>(() =>
                {
                    switch (operation)
                    {
                        case "create": links.Create(null, null); break;
                        case "ensure": links.EnsureCreated(8u); break;
                        case "planner":
                        case "query":
                            AdvancedMixedQueryProcessor.ProcessQuery(links, new()
                            {
                                Query = operation == "planner" ? "(() ((0 0)))" : "(() ((8: 8 8)))"
                            }); break;
                        case "schema": links.StoreTrigger(PersistentTransformationKind.Always, "(() ())"); break;
                        case "import-explicit":
                        case "import-named":
                        case "import-archive": Import(links, operation); break;
                        default: throw new ArgumentException(nameof(operation));
                    }
                });
                Assert.Same(data.Error, error);
                Assert.Equal(before, Rows(links)); Assert.Empty(links.GetTriggers());
                data.Fail = false;
                if (operation.StartsWith("import-", StringComparison.Ordinal))
                {
                    Import(links, operation); var imported = Rows(links);
                    Import(links, operation); Assert.Equal(imported, Rows(links));
                    Assert.Equal("seed 雪", links.GetName(1));
                    var id = operation == "import-named" ? links.GetByName("imported 雪") : 8u;
                    Assert.Equal((id, 1u, operation == "import-named" ? 0u : 8u),
                        Rows(links).Single(row => row.Item1 == id));
                    if (operation != "import-explicit") Assert.Equal("imported 雪", links.GetName(id));
                }
                if (operation == "schema")
                {
                    var root = links.StoreTrigger(PersistentTransformationKind.Always, "(() ())");
                    Assert.Equal(root, links.StoreTrigger(PersistentTransformationKind.Always, "() ()"));
                    Assert.Single(links.GetTriggers());
                    Assert.Equal(1, links.RemoveTriggers("(() ())"));
                }
                following = links.CreateAndUpdate(0u, 0u);
                links.SetName(following, "following 雪=\0");
                expected = Rows(links); data.Flush();
            }
            using (var reopened = new NamedTypesDecorator<uint>(path))
            {
                Assert.Equal(expected, Rows(reopened));
                Assert.Equal("following 雪=\0", reopened.GetName(following));
                if (operation.StartsWith("import-", StringComparison.Ordinal))
                {
                    Assert.Equal("seed 雪", reopened.GetName(1));
                    if (operation != "import-explicit")
                    {
                        var id = operation == "import-named" ? reopened.GetByName("imported 雪") : 8u;
                        Assert.Equal("imported 雪", reopened.GetName(id));
                    }
                }
                var links = new PersistentTransformationDecorator(reopened, reopened);
                Assert.Empty(links.GetTriggers());
                links.RemoveName(following); links.Delete(following);
                Assert.False(links.Exists(following)); reopened.Flush();
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
