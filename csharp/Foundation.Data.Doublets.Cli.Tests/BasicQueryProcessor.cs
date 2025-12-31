using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

using static Foundation.Data.Doublets.Cli.BasicQueryProcessor;

namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
    public class BasicQueryProcessor
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
            Assert.Contains(allLinks, link => link.Index == index && link.Source == source && link.Target == target);
        }
    }
}