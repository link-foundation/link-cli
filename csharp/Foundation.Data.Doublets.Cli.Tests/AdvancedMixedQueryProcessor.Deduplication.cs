// Link deduplication tests of the AdvancedMixedQueryProcessor suite, moved
// out of AdvancedMixedQueryProcessor.More.cs so neither file is above the
// 900-line warning threshold of csharp/scripts/check-file-size.mjs (issue #172).
using Platform.Data;
using Platform.Data.Doublets;

using static Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor;
namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
    public partial class AdvancedMixedQueryProcessor
    {

        [Fact]
        public void DeduplicateDuplicatePairWithNamedLinks_ShouldCreateOnlyOneSubLink()
        {
            // Issue #65: Test deduplication of (m a) (m a) pattern
            // Query: () (((m a) (m a)))
            // Expected: m, a (named self-refs), link 3 = (m a), link 4 = (3 3)
            RunTestWithLinks(links =>
            {
                // Act
                ProcessQuery(links, "(() (((m a) (m a))))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(4, allLinks.Count);

                // Get the named link IDs
                var mId = links.GetByName("m");
                var aId = links.GetByName("a");

                Assert.NotEqual(links.Constants.Null, mId);
                Assert.NotEqual(links.Constants.Null, aId);

                // m and a should be self-referencing
                AssertLinkExists(allLinks, mId, mId, mId);
                AssertLinkExists(allLinks, aId, aId, aId);

                // Find the (m a) link
                var maLink = allLinks.FirstOrDefault(l => l.Source == mId && l.Target == aId);
                Assert.NotEqual(default, maLink);

                // Find the outer link ((m a) (m a)) which should be (maLink.Index maLink.Index)
                var outerLink = allLinks.FirstOrDefault(l => l.Source == maLink.Index && l.Target == maLink.Index);
                Assert.NotEqual(default, outerLink);

                // Verify deduplication: the outer link's source and target should be the same
                Assert.Equal(outerLink.Source, outerLink.Target);
            });
        }

        [Fact]
        public void DeduplicateDuplicatePairWithNumericLinks_ShouldCreateOnlyOneSubLink()
        {
            // Issue #65: Test deduplication with numeric IDs
            // Query: () (((1 2) (1 2)))
            // When using numeric IDs directly, they are treated as references (not creating self-refs)
            // So (1 2) creates link with source=1, target=2
            // The deduplication still works: ((1 2) (1 2)) creates only one (1 2) link
            RunTestWithLinks(links =>
            {
                // Act
                ProcessQuery(links, "(() (((1 2) (1 2))))");

                // Assert
                var allLinks = GetAllLinks(links);

                // Should have 2 links: (1 2) and ((1 2) (1 2))
                Assert.Equal(2, allLinks.Count);

                // Link 1 should be (1 2) - the deduplicated sub-link
                AssertLinkExists(allLinks, 1, 1, 2);

                // Link 2 should be (1 1) - referencing the same sub-link twice
                AssertLinkExists(allLinks, 2, 1, 1);
            });
        }

        [Fact]
        public void DeduplicateTripleDuplicatePair_ShouldCreateOnlyOneSubLink()
        {
            // Test with three identical pairs using named links: (((a b) ((a b) (a b))))
            // The (a b) should only be created once
            RunTestWithLinks(links =>
            {
                // Act
                ProcessQuery(links, "(() (((a b) ((a b) (a b)))))");

                // Assert
                var allLinks = GetAllLinks(links);

                var aId = links.GetByName("a");
                var bId = links.GetByName("b");

                // a and b should be self-referencing
                AssertLinkExists(allLinks, aId, aId, aId);
                AssertLinkExists(allLinks, bId, bId, bId);

                // Find (a b) link - the deduplicated sub-link
                var abLink = allLinks.FirstOrDefault(l => l.Source == aId && l.Target == bId);
                Assert.NotEqual(default, abLink);

                // Find ((a b) (a b)) link - should reference abLink twice
                var innerLink = allLinks.FirstOrDefault(l => l.Source == abLink.Index && l.Target == abLink.Index);
                Assert.NotEqual(default, innerLink);

                // Find outer link ((a b) ((a b) (a b)))
                var outerLink = allLinks.FirstOrDefault(l => l.Source == abLink.Index && l.Target == innerLink.Index);
                Assert.NotEqual(default, outerLink);

                Assert.Equal(5, allLinks.Count);
            });
        }

        [Fact]
        public void DeduplicateMixedNamedAndNumericLinks_ShouldReuseExistingLinks()
        {
            // Test that named links are reused across queries
            RunTestWithLinks(links =>
            {
                // First query creates (m a)
                ProcessQuery(links, "(() ((m a)))");

                var mId = links.GetByName("m");
                var aId = links.GetByName("a");

                // Second query should reuse existing m and a links
                ProcessQuery(links, "(() (((m a) (m a))))");

                // Assert
                var allLinks = GetAllLinks(links);

                // m and a should still have the same IDs
                Assert.Equal(mId, links.GetByName("m"));
                Assert.Equal(aId, links.GetByName("a"));

                // Should have 4 links total: m, a, (m a), ((m a) (m a))
                Assert.Equal(4, allLinks.Count);
            });
        }

        [Fact]
        public void DeduplicateWithDifferentPairs_ShouldNotDeduplicateDifferentLinks()
        {
            // Test that different pairs are NOT deduplicated
            // Query: () (((a b) (b a))) - using named links
            // (a b) and (b a) are different and should both be created
            RunTestWithLinks(links =>
            {
                // Act
                ProcessQuery(links, "(() (((a b) (b a))))");

                // Assert
                var allLinks = GetAllLinks(links);

                var aId = links.GetByName("a");
                var bId = links.GetByName("b");

                // a and b should be self-referencing
                AssertLinkExists(allLinks, aId, aId, aId);
                AssertLinkExists(allLinks, bId, bId, bId);

                // Find (a b) link
                var abLink = allLinks.FirstOrDefault(l => l.Source == aId && l.Target == bId);
                Assert.NotEqual(default, abLink);

                // Find (b a) link
                var baLink = allLinks.FirstOrDefault(l => l.Source == bId && l.Target == aId);
                Assert.NotEqual(default, baLink);

                // Find outer link ((a b) (b a)) - should have different source and target
                var outerLink = allLinks.FirstOrDefault(l => l.Source == abLink.Index && l.Target == baLink.Index);
                Assert.NotEqual(default, outerLink);
                Assert.NotEqual(outerLink.Source, outerLink.Target);

                Assert.Equal(5, allLinks.Count);
            });
        }

        [Fact]
        public void DeduplicateNestedDuplicates_ShouldDeduplicateAtAllLevels()
        {
            // Test deeply nested deduplication using named links
            // Query: () ((((x y) (x y)) ((x y) (x y))))
            // (x y) is duplicated at multiple levels
            RunTestWithLinks(links =>
            {
                // Act
                ProcessQuery(links, "(() ((((x y) (x y)) ((x y) (x y)))))");

                // Assert
                var allLinks = GetAllLinks(links);

                var xId = links.GetByName("x");
                var yId = links.GetByName("y");

                // x and y should be self-referencing
                AssertLinkExists(allLinks, xId, xId, xId);
                AssertLinkExists(allLinks, yId, yId, yId);

                // Find (x y) - the base link
                var xyLink = allLinks.FirstOrDefault(l => l.Source == xId && l.Target == yId);
                Assert.NotEqual(default, xyLink);

                // Find ((x y) (x y)) - references (x y) twice (deduplicated)
                var level1Link = allLinks.FirstOrDefault(l => l.Source == xyLink.Index && l.Target == xyLink.Index);
                Assert.NotEqual(default, level1Link);

                // Find (((x y) (x y)) ((x y) (x y))) - references level1Link twice (deduplicated)
                var level2Link = allLinks.FirstOrDefault(l => l.Source == level1Link.Index && l.Target == level1Link.Index);
                Assert.NotEqual(default, level2Link);

                // Total: x, y, (x y), ((x y) (x y)), (((x y) (x y)) ((x y) (x y)))
                Assert.Equal(5, allLinks.Count);
            });
        }

        [Fact]
        public void DeduplicateNamedLinks_MultipleQueries_ShouldReuseSameIds()
        {
            // Issue #65: Verify that named links maintain consistent IDs across operations
            RunTestWithLinks(links =>
            {
                // First create named links
                ProcessQuery(links, "(() ((p: p p)))");
                ProcessQuery(links, "(() ((a: a a)))");

                var pId = links.GetByName("p");
                var aId = links.GetByName("a");

                // Now create ((p a) (p a)) - should reuse existing p and a
                ProcessQuery(links, "(() (((p a) (p a))))");

                // Assert
                var allLinks = GetAllLinks(links);

                // p and a should still have the same IDs
                Assert.Equal(pId, links.GetByName("p"));
                Assert.Equal(aId, links.GetByName("a"));

                // Verify the structure
                AssertLinkExists(allLinks, pId, pId, pId);
                AssertLinkExists(allLinks, aId, aId, aId);

                // Find (p a) link
                var paLink = allLinks.FirstOrDefault(l => l.Source == pId && l.Target == aId);
                Assert.NotEqual(default, paLink);

                // Find ((p a) (p a)) link - should reference paLink twice
                var outerLink = allLinks.FirstOrDefault(l => l.Source == paLink.Index && l.Target == paLink.Index);
                Assert.NotEqual(default, outerLink);
            });
        }
    }
}
