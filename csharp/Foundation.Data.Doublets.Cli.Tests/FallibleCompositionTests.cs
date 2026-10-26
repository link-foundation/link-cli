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

    [Theory]
    [InlineData("create")]
    [InlineData("ensure")]
    [InlineData("planner")]
    [InlineData("query")]
    [InlineData("schema")]
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
                        default: throw new ArgumentException(nameof(operation));
                    }
                });
                Assert.Same(data.Error, error);
                Assert.Equal(before, Rows(links)); Assert.Empty(links.GetTriggers());
                data.Fail = false;
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
                var links = new PersistentTransformationDecorator(reopened, reopened);
                Assert.Empty(links.GetTriggers());
                links.RemoveName(following); links.Delete(following);
                Assert.False(links.Exists(following)); reopened.Flush();
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
