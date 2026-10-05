// An update into an existing doublet merges into it and re-points the links that used the merged-away
// address, the way the Rust port does (https://github.com/linksplatform/Data.Doublets/issues/515).
using DoubletLink = Platform.Data.Doublets.Link<uint>;

using static Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor;
namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
    public partial class AdvancedMixedQueryProcessor
    {
        [Fact]
        public void UpdateIntoExistingPairRepointsUsagesTest()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "() ((1 2) (2 1))");

                var changes = ReportedChanges(links, "((1: 1 2)) ((1: 2 1))");

                Assert.Equal(
                    new[]
                    {
                        (new DoubletLink(1, 1, 2), new DoubletLink(1, 2, 1)),
                        (new DoubletLink(2, 2, 1), new DoubletLink(2, 2, 2)),
                    },
                    changes);
                Assert.Equal(new[] { new DoubletLink(1, 2, 1), new DoubletLink(2, 2, 2) }, GetAllLinks(links));
            });
        }

        [Fact]
        public void UpdateIntoAPairThatUsesItRepointsTheUsageTest()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "() ((1 1) (2 2))");
                ProcessQuery(links, "((1: 1 1)) ((1: 1 2))");

                var changes = ReportedChanges(links, "((2: 2 2)) ((2: 1 2))");

                Assert.Equal(
                    new[]
                    {
                        (new DoubletLink(1, 1, 2), new DoubletLink(1, 1, 1)),
                        (new DoubletLink(2, 2, 2), new DoubletLink(2, 1, 2)),
                    },
                    changes);
                Assert.Equal(new[] { new DoubletLink(1, 1, 1), new DoubletLink(2, 1, 2) }, GetAllLinks(links));
            });
        }
    }
}
