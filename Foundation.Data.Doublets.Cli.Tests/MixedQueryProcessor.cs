using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

using static Foundation.Data.Doublets.Cli.MixedQueryProcessor;

namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
    public class MixedQueryProcessor
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

        [Fact(Skip = "This test is not working as expected")]
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

        // Helper methods
        private static void RunTestWithLinks(Action<ILinks<uint>> testAction)
        {
            string tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<uint>(tempDbFile);
                testAction(links);
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
            Assert.Contains(allLinks, link => link.Index == index && link.Source == source && link.Target == target);
        }
    }
}