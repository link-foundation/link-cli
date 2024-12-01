using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Protocols.Lino;
using Xunit;

using LinoLink = Platform.Protocols.Lino.Link<string>;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
    public class LinkCliTests
    {
        [Fact]
        public void CreateSingleLinkTest()
        {
            // Arrange
            string tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<uint>(tempDbFile);
                var query = "(() ((1 1)))";

                // Act
                ProcessQuery(links, query);

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Single(allLinks);
                AssertLinkExists(allLinks, index: 1, source: 1, target: 1);
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        [Fact]
        public void CreateLinkWithSource2Target2Test()
        {
            // Arrange
            string tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<uint>(tempDbFile);

                // First, create initial link
                ProcessQuery(links, "(() ((1 1)))");

                var query = "(() ((2 2)))";

                // Act
                ProcessQuery(links, query);

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, index: 1, source: 1, target: 1);
                AssertLinkExists(allLinks, index: 2, source: 2, target: 2);
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        [Fact]
        public void CreateMultipleLinksTest()
        {
            // Arrange
            string tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<uint>(tempDbFile);
                var query = "(() ((1 1) (2 2)))";

                // Act
                ProcessQuery(links, query);

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, index: 1, source: 1, target: 1);
                AssertLinkExists(allLinks, index: 2, source: 2, target: 2);
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        [Fact]
        public void UpdateSingleLinkTest()
        {
            // Arrange
            string tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<uint>(tempDbFile);

                // Create initial links
                ProcessQuery(links, "(() ((1 1)))");
                ProcessQuery(links, "(() ((2 2)))");

                var query = "(((1: 1 1)) ((1: 1 2)))";

                // Act
                ProcessQuery(links, query);

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, index: 1, source: 1, target: 2);
                AssertLinkExists(allLinks, index: 2, source: 2, target: 2);
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        [Fact]
        public void DeleteSingleLinkTest_Source1Target2()
        {
            // Arrange
            string tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<uint>(tempDbFile);

                // Create initial links
                ProcessQuery(links, "(() ((1 2)))");
                ProcessQuery(links, "(() ((2 2)))");

                var query = "(((1 2)) ())";

                // Act
                ProcessQuery(links, query);

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Single(allLinks);
                AssertLinkExists(allLinks, index: 2, source: 2, target: 2);
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        [Fact]
        public void DeleteSingleLinkTest_Source2Target2()
        {
            // Arrange
            string tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<uint>(tempDbFile);

                // Create initial link
                ProcessQuery(links, "(() ((2 2)))");

                var query = "(((2 2)) ())";

                // Act
                ProcessQuery(links, query);

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Empty(allLinks);
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        [Fact]
        public void DeleteMultipleLinksTest()
        {
            // Arrange
            string tempDbFile = Path.GetTempFileName();
            try
            {
                using var links = new UnitedMemoryLinks<uint>(tempDbFile);

                // Create initial links
                ProcessQuery(links, "(() ((1 2) (2 2)))");

                var query = "(((1 2) (2 2)) ())";

                // Act
                ProcessQuery(links, query);

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Empty(allLinks);
            }
            finally
            {
                File.Delete(tempDbFile);
            }
        }

        // Helper methods

        private static List<DoubletLink> GetAllLinks(ILinks<uint> links)
        {
            var allLinks = new List<DoubletLink>();
            var any = links.Constants.Any;
            var query = new DoubletLink(index: any, source: any, target: any);
            return links.All(query).Select(doublet => new DoubletLink(doublet)).ToList();
        }

        private static void AssertLinkExists(List<DoubletLink> allLinks, uint index, uint source, uint target)
        {
            Assert.Contains(allLinks, link => link.Index == index && link.Source == source && link.Target == target);
        }

        // Reusing the ProcessQuery function from your main code
        private static void ProcessQuery(ILinks<uint> links, string query)
        {
            var parser = new Parser();
            var parsedLinks = parser.Parse(query);

            if (parsedLinks.Count == 0)
            {
                return;
            }

            var outerLink = parsedLinks[0];
            var outerLinkValues = outerLink.Values;

            if (outerLinkValues?.Count < 2)
            {
                return;
            }

            var @null = links.Constants.Null;
            var any = links.Constants.Any;

            var restrictionLink = outerLinkValues[0];
            var substitutionLink = outerLinkValues[1];

            if ((restrictionLink.Values?.Count == 0) &&
                (substitutionLink.Values?.Count == 0))
            {
                return;
            }
            else if ((restrictionLink.Values?.Count > 0) &&
                     (substitutionLink.Values?.Count > 0))
            {
                // Update operation (only single link is supported at the moment)
                var restrictionDoublet = ToDoubletLink(links, restrictionLink.Values[0], any);
                var substitutionDoublet = ToDoubletLink(links, substitutionLink.Values[0], @null);

                links.Update(restrictionDoublet, substitutionDoublet, (before, after) =>
                {
                    return links.Constants.Continue;
                });

                return;
            }
            else if (substitutionLink.Values?.Count == 0) // If substitution is empty, perform delete operation
            {
                foreach (var linkToDelete in restrictionLink.Values ?? Array.Empty<LinoLink>())
                {
                    var queryLink = ToDoubletLink(links, linkToDelete, any);
                    links.DeleteByQuery(queryLink);
                }
                return;
            }
            else if (restrictionLink.Values?.Count == 0) // If restriction is empty, perform create operation
            {
                foreach (var linkToCreate in substitutionLink.Values ?? Array.Empty<LinoLink>())
                {
                    var doubletLink = ToDoubletLink(links, linkToCreate, @null);
                    links.GetOrCreate(doubletLink.Source, doubletLink.Target);
                }
                return;
            }
        }

        private static DoubletLink ToDoubletLink(ILinks<uint> links, LinoLink linoLink, uint defaultValue)
        {
            uint index = defaultValue;
            uint source = defaultValue;
            uint target = defaultValue;
            if (!string.IsNullOrEmpty(linoLink.Id) && uint.TryParse(linoLink.Id, out uint linkId))
            {
                index = linkId;
            }
            if (linoLink.Values?.Count == 2)
            {
                var sourceLink = linoLink.Values[0];
                var targetLink = linoLink.Values[1];
                if (!string.IsNullOrEmpty(sourceLink.Id) && uint.TryParse(sourceLink.Id, out uint sourceId))
                {
                    source = sourceId;
                }
                if (!string.IsNullOrEmpty(targetLink.Id) && uint.TryParse(targetLink.Id, out uint targetId))
                {
                    target = targetId;
                }
            }
            return new DoubletLink(index, source, target);
        }
    }
}