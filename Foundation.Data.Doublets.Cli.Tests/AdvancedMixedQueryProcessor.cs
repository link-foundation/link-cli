using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

using static Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor;

namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
    public class AdvancedMixedQueryProcessor
    {
        [Fact]
        public void CreateSingleLinkTest()
        {
            RunTestWithLinks(links =>
            {
                // Act
                ProcessQuery(links, "(() ((1 1)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Single(allLinks);
                AssertLinkExists(allLinks, 1, 1, 1);
            });
        }

        [Fact]
        public void CreateSingleLinkWithIndexTest()
        {
            RunTestWithLinks(links =>
            {
                // Act
                ProcessQuery(links, "(() ((1: 1 1)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Single(allLinks);
                AssertLinkExists(allLinks, 1, 1, 1);
            });
        }

        [Fact]
        public void CreateSingleLinkWithIndexAfterGapTest()
        {
            RunTestWithLinks(links =>
            {
                // Act
                ProcessQuery(links, "(() ((2: 2 2)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Single(allLinks);
                AssertLinkExists(allLinks, 2, 2, 2);
            });
        }

        [Fact]
        public void CreateSingleLinkWithIndexAfterDoubleGapTest()
        {
            RunTestWithLinks(links =>
            {
                // Act
                ProcessQuery(links, "(() ((3: 3 3)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Single(allLinks);
                AssertLinkExists(allLinks, 3, 3, 3);
            });
        }

        [Fact]
        public void CreateLinkWithSource2Target2Test()
        {
            RunTestWithLinks(links =>
            {
                // Act
                ProcessQuery(links, "(() ((1 1)))");
                ProcessQuery(links, "(() ((2 2)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 1, 1);
                AssertLinkExists(allLinks, 2, 2, 2);
            });
        }

        [Fact]
        public void CreateMultipleLinksTest()
        {
            RunTestWithLinks(links =>
            {
                // Act
                ProcessQuery(links, "(() ((1 1) (2 2)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 1, 1);
                AssertLinkExists(allLinks, 2, 2, 2);
            });
        }

        [Fact]
        public void UpdateSingleLinkTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 1)))");
                ProcessQuery(links, "(() ((2 2)))");

                // Act
                ProcessQuery(links, "(((1: 1 1)) ((1: 1 2)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 1, 2);
                AssertLinkExists(allLinks, 2, 2, 2);
            });
        }

        [Fact]
        public void NoUpdateUsingVariablesTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 1)))");
                ProcessQuery(links, "(() ((2 2)))");

                Options options = new Options();

                var changes = new List<(DoubletLink, DoubletLink)>();
                options.Query = "((($index: $source $target)) (($index: $source $target)))";
                options.ChangesHandler = (before, after) =>
                {
                    changes.Add((new DoubletLink(before), new DoubletLink(after)));
                    return links.Constants.Continue;
                };

                // Act
                ProcessQuery(links, options);

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 1, 1);
                AssertLinkExists(allLinks, 2, 2, 2);
                Assert.Equal(2, changes.Count);
                AssertChangeExists(changes, new DoubletLink(1, 1, 1), new DoubletLink(1, 1, 1));
                AssertChangeExists(changes, new DoubletLink(2, 2, 2), new DoubletLink(2, 2, 2));
            });
        }

        [Fact]
        public void SwapSourceAndTargetForSingleLinkUsingVariablesTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 1)))");
                ProcessQuery(links, "(() ((1 2)))");

                // Act
                ProcessQuery(links, "(((2: $source $target)) ((2: $target $source)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 1, 1);
                AssertLinkExists(allLinks, 2, 2, 1);
            });
        }

        [Fact]
        public void SwapSourceAndTargetForAllLinksUsingVariablesTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange: create initial links (1: 1 2) and (2: 2 1)
                ProcessQuery(links, "(() ((1 2) (2 1)))");

                // Act: swap source and target for all links
                ProcessQuery(links, "((($index: $source $target)) (($index: $target $source)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 2, 1);
                AssertLinkExists(allLinks, 2, 1, 2);
            });
        }

        [Fact]
        public void MakeAllLinksToGoOutOfFirstLinkUsingVariablesTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((2 2) (2 1)))");

                // Act: make all links to go out of the first link
                ProcessQuery(links, "((($index: $source $target)) (($index: 1 $target)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 1, 2);
                AssertLinkExists(allLinks, 2, 1, 1);
            });
        }

        [Fact]
        public void MakeAllLinksToGoIntoFirstLinkUsingVariablesTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((2 2) (1 2)))");

                // Act: make all links to go into the first link
                ProcessQuery(links, "((($index: $source $target)) (($index: $source 1)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 2, 1);
                AssertLinkExists(allLinks, 2, 1, 1);
            });
        }

        [Fact]
        public void MakeAllLinksUniqueSelfReferencingUsingVariablesTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 2) (2 1)))");

                // Act: make all links unique self-referencing
                ProcessQuery(links, "((($index: $source $target)) (($index: $index $index)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 1, 1);
                AssertLinkExists(allLinks, 2, 2, 2);
            });
        }

        [Fact]
        public void MultipleUpdatesTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 1) (2 2)))");

                // Act
                ProcessQuery(links, "(((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 1, 2);
                AssertLinkExists(allLinks, 2, 2, 1);
            });
        }

        [Fact]
        public void MixedMultipleUpdatesTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 1) (2 2)))");

                // Act
                ProcessQuery(links, "(((2: 2 2) (1: 1 1)) ((1: 1 2) (2: 2 1)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 1, 2);
                AssertLinkExists(allLinks, 2, 2, 1);
            });
        }

        [Fact]
        public void CreationDuringUpdateTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 1)))");

                // Act: Add new link with ID '2' by including it only in substitution
                ProcessQuery(links, "(((1: 1 1)) ((1: 1 1) (2: 2 2)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 1, 1);
                AssertLinkExists(allLinks, 2, 2, 2);
            });
        }

        [Fact]
        public void CreationWithEmptySlotDuringUpdateTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 1)))");

                // Act: Add new link with ID '2' by including it only in substitution
                ProcessQuery(links, "(((1: 1 1)) ((1: 1 1) (3: 3 3)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 1, 1);
                AssertLinkExists(allLinks, 3, 3, 3);
            });
        }

        [Fact]
        public void DeletionDuringUpdateTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 1) (2 2)))");

                // Act: Remove link with ID '2' by omitting it in substitution
                ProcessQuery(links, "(((1: 1 1) (2: 2 2)) ((1: 1 1)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Single(allLinks);
                AssertLinkExists(allLinks, 1, 1, 1);
            });
        }

        [Fact]
        public void DeleteSingleLinkTest_Source1Target2()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 2)))");
                ProcessQuery(links, "(() ((2 2)))");

                // Act
                ProcessQuery(links, "(((1 2)) ())");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Single(allLinks);
                AssertLinkExists(allLinks, 2, 2, 2);
            });
        }

        [Fact]
        public void DeleteSingleLinkTest_Source2Target2()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((2 2)))");

                // Act
                ProcessQuery(links, "(((2 2)) ())");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Empty(allLinks);
            });
        }

        [Fact]
        public void DeleteMultipleLinksTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 2) (2 2)))");

                // Act
                ProcessQuery(links, "(((1 2) (2 2)) ())");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Empty(allLinks);
            });
        }

        [Fact]
        public void DeleteLinksByAnyTargetTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 2) (2 2)))");

                // Act
                ProcessQuery(links, "(((1 *)) ())");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Single(allLinks);
                AssertLinkExists(allLinks, 2, 2, 2);
            });
        }

        [Fact]
        public void DeleteLinksByAnySourceTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 1) (1 2)))");

                // Act
                ProcessQuery(links, "(((* 2)) ())");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Single(allLinks);
                AssertLinkExists(allLinks, 1, 1, 1);
            });
        }

        [Fact]
        public void DeleteAllLinksBySourceAndTargetTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 2) (2 2)))");

                // Act
                ProcessQuery(links, "(((* *)) ())");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Empty(allLinks);
            });
        }

        [Fact]
        public void DeleteAllLinksByIndexTest()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 2) (2 2)))");

                // Act
                ProcessQuery(links, "(((*:)) ())");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Empty(allLinks);
            });
        }

        // Helper methods
        private static void RunTestWithLinks(Action<ILinks<uint>> testAction)
        {
            string tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<uint>(tempDbFile);
                var decoratedLinks = links.DecorateWithAutomaticUniquenessAndUsagesResolution();
                testAction(decoratedLinks);
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        private static List<DoubletLink> GetAllLinks(ILinks<uint> links)
        {
            var any = links.Constants.Any;
            var query = new DoubletLink(index: any, source: any, target: any);
            return links.All(query).Select(doublet => new DoubletLink(doublet)).ToList();
        }

        private static void AssertLinkExists(List<DoubletLink> allLinks, uint index, uint source, uint target)
        {
            var link = new DoubletLink(index, source, target);
            Assert.True(allLinks.Contains(link), $"Link {link} not found in the list of all links ({string.Join(" ", allLinks)})");
        }

        private static void AssertChangeExists(List<(DoubletLink, DoubletLink)> changes, DoubletLink linkBefore, DoubletLink linkAfter)
        {
            Assert.Contains(changes, change => change.Item1 == linkBefore && change.Item2 == linkAfter);
        }
    }
}