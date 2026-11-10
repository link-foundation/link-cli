using Platform.Data.Doublets;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests;

[Collection("Diagnostic console")]
public sealed class QueryCompatibilityContractTests
{
    public static IEnumerable<object[]> NoOpQueries()
    {
        foreach (var processor in new[] { "basic", "mixed", "advanced" })
            foreach (var query in new[] { "", " \t\r\n", "# only a comment\n", "1", "plain", "'雪'", "()", "(1)", "(() ())" })
                yield return [processor, query];
    }

    private static void Process(string processor, NamedTypesDecorator<uint> links, string query, List<(DoubletLink, DoubletLink)> changes)
    {
        uint Observe(IList<uint>? before, IList<uint>? after)
        {
            changes.Add((new DoubletLink(before), new DoubletLink(after)));
            return links.Constants.Continue;
        }
        switch (processor)
        {
            case "basic": BasicQueryProcessor.ProcessQuery(links, query); break;
            case "mixed": MixedQueryProcessor.ProcessQuery(links, new() { Query = query, ChangesHandler = Observe }); break;
            case "advanced":
                AdvancedMixedQueryProcessor.ProcessQuery(links, new()
                {
                    Query = query,
                    ChangesHandler = Observe,
                    AutoCreateMissingReferences = true
                }); break;
            default: throw new ArgumentException("Unknown test processor.", nameof(processor));
        }
    }

    private static (uint Index, uint Source, uint Target)[] Rows(NamedTypesDecorator<uint> links) =>
        links.All(new DoubletLink(links.Constants.Any, links.Constants.Any, links.Constants.Any))
            .Select(row => new DoubletLink(row)).OrderBy(row => row.Index)
            .Select(row => (row.Index, row.Source, row.Target)).ToArray();

    private static void WithLinks(Action<NamedTypesDecorator<uint>> action)
    {
        var directory = Directory.CreateTempSubdirectory("query-rejection-").FullName;
        try
        {
            using var links = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            Assert.Equal(1u, links.CreateAndUpdate(1u, 1u));
            links.SetName(1u, "kept=雪");
            action(links);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void MissingOwnerOrOptionsAreArgumentErrorsAndDoNotChangeFollowingQueries(bool advanced)
    {
        WithLinks(links =>
        {
            var missingLinks = Assert.Throws<ArgumentNullException>(() =>
            {
                if (advanced) AdvancedMixedQueryProcessor.ProcessQuery(null!, new());
                else MixedQueryProcessor.ProcessQuery(null!, new());
            });
            Assert.Equal("links", missingLinks.ParamName);
            var missingOptions = Assert.Throws<ArgumentNullException>(() =>
            {
                if (advanced) AdvancedMixedQueryProcessor.ProcessQuery(links, null!);
                else MixedQueryProcessor.ProcessQuery(links, null!);
            });
            Assert.Equal("options", missingOptions.ParamName);
            Assert.Equal(new[] { (1u, 1u, 1u) }, Rows(links));
            Assert.Equal("kept=雪", links.GetName(1u));
            Process(advanced ? "advanced" : "mixed", links, "(() ((2 2)))", []);
            Assert.Equal(new[] { (1u, 1u, 1u), (2u, 2u, 2u) }, Rows(links));
        });
    }

    [Theory]
    [InlineData("(() ((1: * 1)))", "source")]
    [InlineData("(() ((1: 1 *)))", "target")]
    public void RejectedMixedWildcardCreationPreservesTheExistingRowAndName(string query, string position)
    {
        WithLinks(links =>
        {
            var changes = new List<(DoubletLink, DoubletLink)>();
            var error = Assert.Throws<ArgumentException>(() => Process("mixed", links, query, changes));
            Assert.Contains($"The {position} of the link", error.Message, StringComparison.Ordinal);
            Assert.Empty(changes);
            Assert.Equal(new[] { (1u, 1u, 1u) }, Rows(links));
            Assert.Equal("kept=雪", links.GetName(1u));
            Process("mixed", links, "(() ((2 2)))", changes);
            Assert.Equal(new[] { (1u, 1u, 1u), (2u, 2u, 2u) }, Rows(links));
        });
    }

    [Fact]
    public void ExistingMixedPairReportsItsUnchangedStateWithoutAllocatingAnotherRow()
    {
        WithLinks(links =>
        {
            var changes = new List<(DoubletLink, DoubletLink)>();
            Process("mixed", links, "(() ((1 1)))", changes);
            var change = Assert.Single(changes);
            Assert.Equal(new DoubletLink(1u, 1u, 1u), change.Item1);
            Assert.Equal(change.Item1, change.Item2);
            Assert.Equal(new[] { (1u, 1u, 1u) }, Rows(links));
            Assert.Equal("kept=雪", links.GetName(1u));
            Process("mixed", links, "(() ((2 2)))", changes);
            Assert.Equal(new[] { (1u, 1u, 1u), (2u, 2u, 2u) }, Rows(links));
        });
    }

    [Fact]
    public void BasicMismatchedUpdateCountsReportTheProblemBeforeChangingRows()
    {
        WithLinks(links =>
        {
            Process("basic", links, "(() ((2 2)))", []);
            var previous = Console.Out;
            using var diagnostic = new StringWriter();
            try
            {
                Console.SetOut(diagnostic);
                Process("basic", links, "(((1: 1 1) (2: 2 2)) ((1: 1 2)))", []);
            }
            finally { Console.SetOut(previous); }
            Assert.Equal("The number of restriction links and substitution links must be the same.", diagnostic.ToString().Trim());
            Assert.Equal(new[] { (1u, 1u, 1u), (2u, 2u, 2u) }, Rows(links));
            Assert.Equal("kept=雪", links.GetName(1u));
            Process("basic", links, "(() ((3 3)))", []);
            Assert.Equal(new[] { (1u, 1u, 1u), (2u, 2u, 2u), (3u, 3u, 3u) }, Rows(links));
        });
    }

    [Theory]
    [MemberData(nameof(NoOpQueries))]
    public void NoOpShapePreservesRowsNamesAndNotificationsAndAllowsFollowingReopenedEdits(string processor, string query)
    {
        var directory = Directory.CreateTempSubdirectory("query-compatibility-").FullName;
        var path = Path.Combine(directory, "data.links");
        try
        {
            using (var links = new NamedTypesDecorator<uint>(path))
            {
                Assert.Equal(1u, links.CreateAndUpdate(1u, 1u));
                links.SetName(1u, "kept=雪");
                var changes = new List<(DoubletLink, DoubletLink)>();
                Process(processor, links, query, changes);
                Assert.Equal(new[] { (1u, 1u, 1u) }, Rows(links));
                Assert.Equal("kept=雪", links.GetName(1u));
                Assert.Empty(changes);
                Process(processor, links, "(() ((2 2)))", changes);
                Assert.Equal(new[] { (1u, 1u, 1u), (2u, 2u, 2u) }, Rows(links));
            }
            using (var reopened = new NamedTypesDecorator<uint>(path))
            {
                Assert.Equal(new[] { (1u, 1u, 1u), (2u, 2u, 2u) }, Rows(reopened));
                Assert.Equal("kept=雪", reopened.GetName(1u));
                Process(processor, reopened, "(() ((3 3)))", []);
                Assert.Equal(new[] { (1u, 1u, 1u), (2u, 2u, 2u), (3u, 3u, 3u) }, Rows(reopened));
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
