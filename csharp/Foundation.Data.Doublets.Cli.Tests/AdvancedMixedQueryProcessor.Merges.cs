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

        [Fact]
        public void UpdateIntoAnUntouchedPairReportsOnlyTheMergedLinkTest()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "() ((1 1) (2 2))");

                // (1: 1 1) is where (2) merges, but the update does not change it, so it is not reported
                // (as in the Rust port), although the store reports it since Platform.Data.Doublets 0.18.6.
                var changes = ReportedChanges(links, "((2: 2 2)) ((2: 1 1))");

                Assert.Equal(new[] { (new DoubletLink(2, 2, 2), default(DoubletLink)) }, changes);
                Assert.Equal(new[] { new DoubletLink(1, 1, 1) }, GetAllLinks(links));
            });
        }
    }
}
