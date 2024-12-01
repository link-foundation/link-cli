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

using static Foundation.Data.Doublets.Cli.QueryProcessor;

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
    }
}