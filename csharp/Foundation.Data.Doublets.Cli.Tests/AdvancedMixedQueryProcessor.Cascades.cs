// Deleting a link deletes every link that uses it, transitively: a link never refers to an address that does not
// exist. The Rust port does the same (test_delete_cascade_chain_matches_csharp).
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
    public partial class AdvancedMixedQueryProcessor
    {
        [Fact]
        public void DeleteCascadesToUsagesTest()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "() ((1 1) (2 2) (1 2))");

                var changes = ReportedChanges(links, "((2: 2 2)) ()");

                Assert.Equal(
                    new[]
                    {
                        (new DoubletLink(2, 2, 2), default(DoubletLink)),
                        (new DoubletLink(3, 1, 2), default(DoubletLink)),
                    },
                    changes.OrderBy(change => change.Item1.Index));
                Assert.Equal(new[] { new DoubletLink(1, 1, 1) }, GetAllLinks(links));
            });
        }

        [Fact]
        public void DeleteCascadesThroughAChainOfUsagesTest()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "() ((1 1) (2 2) (1 2))");
                ProcessQuery(links, "() ((3 3))");

                var changes = ReportedChanges(links, "((1: 1 1)) ()");

                Assert.Equal(new uint[] { 1, 3, 4 }, changes.Select(change => change.Item1.Index).Order());
                Assert.All(changes, change => Assert.True(change.Item2.IsNull()));
                Assert.Equal(new[] { new DoubletLink(2, 2, 2) }, GetAllLinks(links));
            });
        }
    }
}
