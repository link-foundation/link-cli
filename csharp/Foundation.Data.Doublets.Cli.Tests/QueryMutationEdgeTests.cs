// Edges of the mixed query processors that ordinary queries do not reach: quoted
// references, moves onto a link already in place and bindings that disagree.
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Memory;
using static Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor<uint>;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
    public sealed class QueryMutationEdgeTests
    {
        private const string OneAndItsTarget = "(() ((1: 1 1) (2: 1 2)))";

        private static string Rows(ILinks<uint> links)
        {
            var rows = new List<string>();
            links.Each(new DoubletLink(links.Constants.Any, links.Constants.Any, links.Constants.Any), row =>
            {
                var link = new DoubletLink(row);
                rows.Add($"{link.Index}:{link.Source}>{link.Target}");
                return links.Constants.Continue;
            });
            return string.Join(" ", rows);
        }

        private static void WithStore(Action<NamedTypesDecorator<uint>> use)
        {
            var data = Path.GetTempFileName();
            var names = Path.GetTempFileName();
            try
            {
                using var memory = new FileMappedResizableDirectMemory(data, UnitedMemoryLinks<uint>.DefaultLinksSizeStep);
                using var united = new UnitedMemoryLinks<uint>(memory);
                using var named = new NamedTypesDecorator<uint>(united.DecorateWithAutomaticUniquenessAndUsagesRepointing(), names);
                use(named);
            }
            finally
            {
                File.Delete(data);
                File.Delete(names);
            }
        }

        private static string After(bool autoCreate, params string[] queries)
        {
            var rows = "";
            WithStore(named =>
            {
                foreach (var query in queries)
                {
                    ProcessQuery(named, new Options { Query = query, AutoCreateMissingReferences = autoCreate });
                }
                rows = Rows(named);
            });
            return rows;
        }

        [Fact]
        public void MovingOntoALinkAlreadyInPlaceReportsItUnchanged()
        {
            WithStore(named =>
            {
                ProcessQuery(named, new Options { Query = "(() ((1: 1 1) (2: 2 2) (3: 3 3)))" });
                var changes = new List<(DoubletLink Before, DoubletLink After)>();
                ProcessQuery(named, new Options
                {
                    Query = "(((1: 1 1)) ((3: 3 3)))",
                    ChangesHandler = (before, after) =>
                    {
                        changes.Add((new DoubletLink(before!), new DoubletLink(after!)));
                        return named.Constants.Continue;
                    },
                });
                Assert.Equal("2:2>2 3:3>3", Rows(named));
                var three = new DoubletLink(3u, 3u, 3u);
                Assert.Contains((three, three), changes);
            });
        }

        [Fact]
        public void TwoLinksCanSwapTheirParts() =>
            Assert.Equal(
                "1:2>2 2:1>1",
                After(false, "(() ((1: 1 1) (2: 2 2)))", "(((1: $a $a) (2: $b $b)) ((1: $b $b) (2: $a $a)))"));

        // Existing behaviour, kept for compatibility: an empty reference means any.
        [Fact]
        public void AnEmptyReferenceInACreatedLinkMeansAny() =>
            WithStore(named =>
            {
                ProcessQuery(named, new Options { Query = "(() ((() 1)))" });
                Assert.Equal($"1:{named.Constants.Any}>1", Rows(named));
            });

        [Fact]
        public void AnyIsNotAnAddressALinkCanBeCreatedAt() =>
            WithStore(named =>
            {
                ProcessQuery(named, new Options { Query = "(() ((1: 1 1) (2: 2 2)))" });
                var refused = Assert.Throws<InvalidOperationException>(() =>
                    ProcessQuery(named, new Options { Query = "(() ((*: 1 2)))" }));
                Assert.Contains("unsupported link address", refused.Message);
                Assert.Equal("1:1>1 2:2>2", Rows(named));
            });

        [Theory]
        // A substitution without an index goes to a link with those parts, if there is one.
        [InlineData("(((2: 1 2)) ((1 1)))", "1:1>1")]
        [InlineData("(((2: 1 2)) ((1 2)))", "1:1>1 2:1>2")]
        [InlineData("(((1: 1 1) (2: 1 2)) ((1 2)))", "1:1>2")]
        [InlineData("(((1: 1 1) (2: 1 2)) ((2 2)))", "1:2>2")]
        [InlineData("(((1: 1 1) (2: 1 2)) ((1: 1 1)))", "1:1>1")]
        // A nested pattern whose index is not the link it reaches matches nothing.
        [InlineData("(((2: (2: 1 1) 2)) ((2: 2 2)))", "1:1>1 2:1>2")]
        // A variable that nothing bound takes no address of its own.
        [InlineData("(((1: 1 1)) (($new: 1 2)))", "1:1>2")]
        // Moving to a free address, and onto a link the same query deleted.
        [InlineData("(((1: 1 1)) ((3: 1 1)))", "3:1>1")]
        [InlineData("(((2: 1 2)) ((3: 2 2)))", "1:1>1 3:2>2")]
        [InlineData("(((2: 1 2)) ((1: 2 2)))", "1:2>2")]
        [InlineData("(((2: 1 2)) (()))", "1:1>1")]
        public void ExplicitAddressesAndPartsDecideWhereARowEndsUp(string query, string expected)
        {
            Assert.Equal(expected, After(true, OneAndItsTarget, query));
        }

        [Theory]
        [InlineData("(('1:' 1 1))")]
        [InlineData("(('nope:' 1 2))")]
        public void AQuotedAddressSuffixIsNotAReferenceUnlessCreated(string restriction) =>
            WithStore(named =>
            {
                ProcessQuery(named, new Options { Query = OneAndItsTarget });
                var refused = Assert.Throws<InvalidOperationException>(() =>
                    ProcessQuery(named, new Options { Query = $"({restriction} ((1: 2 2)))" }));
                Assert.Contains("does not exist", refused.Message);
                Assert.Equal("1:1>1 2:1>2", Rows(named));
            });

        [Theory]
        // A quoted reference is a name, and no link has it, so the row is matched by its parts.
        [InlineData("(() ((1: 1 1) (2: 1 2)))", "((('1:' 1 1)) ((1: 2 2)))", "1:2>2")]
        [InlineData("(() ((1: 1 1) (2: 1 2)))", "((('nope:' 1 2)) ((2: 2 2)))", "2:2>2")]
        [InlineData("(() ((n: n n)))", "((('n:' 1 1)) ((1: 1 1)))", "1:1>1")]
        // In a substitution the quoted text names a new point link.
        [InlineData("(() ((1: 1 1) (2: 1 2)))", "(((2: 1 2)) ((2: '1:' 2)))", "1:1>1 2:3>2 3:3>3")]
        public void AQuotedAddressSuffixResolvesOnceReferencesAreCreated(string setup, string query, string expected) =>
            Assert.Equal(expected, After(true, setup, query));

        [Fact]
        public void BindingsThatDisagreeMatchNothing() =>
            Assert.Equal("1:1>1 2:1>2", After(false, OneAndItsTarget, "(((1: $a $b) (2: $b 1)) ((1: $a $b)))"));

        [Fact]
        public void ANestedPartThatDiffersMatchesNothing() =>
            Assert.Equal("1:1>1 2:1>2", After(false, OneAndItsTarget, "(((2: (1: 2 2) 2)) ())"));

        [Fact]
        public void AnUnboundVariableInASubstitutionKeepsTheStoredPart() =>
            Assert.Equal("1:1>1 2:2>2", After(false, OneAndItsTarget, "(((2: $a $b)) ((2: $b $c)))"));

        [Fact]
        public void TheOlderProcessorIgnoresAQueryOfTwoPlainNames()
        {
            var data = Path.GetTempFileName();
            try
            {
                using var memory = new FileMappedResizableDirectMemory(data, UnitedMemoryLinks<uint>.DefaultLinksSizeStep);
                using var united = new UnitedMemoryLinks<uint>(memory);
                var links = united.DecorateWithAutomaticUniquenessAndUsagesRepointing();
                Foundation.Data.Doublets.Cli.MixedQueryProcessor.ProcessQuery(links, new Foundation.Data.Doublets.Cli.MixedQueryProcessor.Options { Query = "(1 2)" });
                Assert.Equal("", Rows(links));
            }
            finally
            {
                File.Delete(data);
            }
        }
    }
}
