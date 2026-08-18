using System;
using System.IO;
using System.Linq;
using Platform.Data.Doublets;
using Xunit;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests
{
    public class Issue62ReviewCoverageTests
    {
        [Fact]
        public void ExplicitNumericIdUpdate_CanBeReversedWithAnotherUpdate()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "(() ((1: 1 1)))");
                AssertLink(links, 1, 1, 1);

                ProcessQuery(links, "(((1: 1 1)) ((1: 2 2)))");
                AssertLink(links, 1, 2, 2);

                ProcessQuery(links, "(((1: 2 2)) ((1: 1 1)))");
                AssertLink(links, 1, 1, 1);
            });
        }

        [Fact]
        public void NamedLink_CreateDeleteRecreate_DoesNotLeaveStaleNameMapping()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "(() ((child: father mother)))");
                var firstChild = links.GetByName("child");
                Assert.NotEqual(links.Constants.Null, firstChild);

                ProcessQuery(links, "((child: father mother)) ()");
                Assert.Equal(links.Constants.Null, links.GetByName("child"));
                Assert.Null(links.GetName(firstChild));

                ProcessQuery(links, "(() ((child: father mother)))");
                var recreatedChild = links.GetByName("child");
                Assert.NotEqual(links.Constants.Null, recreatedChild);
                Assert.Equal("child", links.GetName(recreatedChild));
            });
        }

        private static void RunTestWithLinks(Action<NamedTypesDecorator<uint>> testAction)
        {
            var tempDbFile = Path.GetTempFileName();
            var namesDbFile = NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(tempDbFile);
            try
            {
                // Disposed at the end of the try block, before the finally deletes the backing files:
                // Windows refuses to delete a file that is still memory-mapped.
                using var links = new NamedTypesDecorator<uint>(tempDbFile);
                testAction(links);
            }
            finally
            {
                if (File.Exists(namesDbFile))
                {
                    File.Delete(namesDbFile);
                }
                if (File.Exists(tempDbFile))
                {
                    File.Delete(tempDbFile);
                }
            }
        }

        private static void ProcessQuery(NamedTypesDecorator<uint> links, string query)
        {
            AdvancedMixedQueryProcessor.ProcessQuery(
              links,
              new AdvancedMixedQueryProcessor.Options
              {
                  Query = query,
                  AutoCreateMissingReferences = true
              });
        }

        private static void AssertLink(NamedTypesDecorator<uint> links, uint index, uint source, uint target)
        {
            var any = links.Constants.Any;
            var allLinks = links.All(new DoubletLink(any, any, any))
              .Select(link => new DoubletLink(link))
              .ToList();

            var formattedLinks = string.Join(" ", allLinks.Select(link => $"({link.Index}: {link.Source}->{link.Target})"));
            Assert.True(
              allLinks.Any(link => link.Index == index && link.Source == source && link.Target == target),
              $"Expected link ({index}: {source}->{target}) but found: {formattedLinks}");
        }
    }
}
