// Shared fixtures and assertions for the AdvancedMixedQueryProcessor tests.
using System.Globalization;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

using static Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor;
namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
    public partial class AdvancedMixedQueryProcessor
    {
        /// </summary>
        private static TimeSpan TestTimeout
        {
            get
            {
                const int defaultTimeoutSeconds = 60;
                var configured = Environment.GetEnvironmentVariable("LINK_CLI_TEST_TIMEOUT_SECONDS");
                if (!string.IsNullOrWhiteSpace(configured)
                    && int.TryParse(configured, NumberStyles.Integer, CultureInfo.InvariantCulture, out var seconds)
                    && seconds > 0)
                {
                    return TimeSpan.FromSeconds(seconds);
                }
                return TimeSpan.FromSeconds(defaultTimeoutSeconds);
            }
        }

        private static void RunTestWithLinks(Action<NamedTypesDecorator<uint>> testAction, bool enableTracing = false)
        {
            string tempDbFile = Path.GetTempFileName();
            var namesDbFile = NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(tempDbFile);
            try
            {
                // Disposed at the end of the try block, before the finally deletes the backing files. The
                // decorator owns memory-mapped handles for both databases; POSIX tolerates unlinking a file
                // that is still mapped, Windows fails the delete with IOException.
                using var decoratedLinks = new NamedTypesDecorator<uint>(tempDbFile, tracingEnabled: enableTracing);

                var timeout = TestTimeout;
                using var cts = new CancellationTokenSource(timeout);
                var task = Task.Run(() =>
                {
                    testAction(decoratedLinks);
                }, cts.Token);

                try
                {
                    task.Wait(cts.Token);
                }
                catch (OperationCanceledException)
                {
                    Console.WriteLine($"[Test] Test was cancelled after {timeout.TotalSeconds} seconds timeout");
                    throw new TimeoutException($"Test exceeded {timeout.TotalSeconds} seconds timeout");
                }
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

        private static List<DoubletLink> GetAllLinks(NamedTypesDecorator<uint> links)
        {
            var any = links.Constants.Any;
            var query = new DoubletLink(index: any, source: any, target: any);
            var allLinks = links.All(query).Select(doublet => new DoubletLink(doublet)).ToList();
            Console.WriteLine($"[Test] All links: {string.Join(" ", allLinks)}");
            return allLinks;
        }

        private static void ProcessQuery(NamedTypesDecorator<uint> links, string query)
        {
            ProcessQuery(links, new Options { Query = query });
        }

        private static void ProcessQuery(NamedTypesDecorator<uint> links, Options options)
        {
            options.AutoCreateMissingReferences = true;
            Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor.ProcessQuery(links, options);
        }

        private static void ProcessQueryStrict(NamedTypesDecorator<uint> links, string query)
        {
            ProcessQueryStrict(links, new Options { Query = query });
        }

        private static void ProcessQueryStrict(NamedTypesDecorator<uint> links, Options options)
        {
            options.AutoCreateMissingReferences = false;
            Foundation.Data.Doublets.Cli.AdvancedMixedQueryProcessor.ProcessQuery(links, options);
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

        // New tests for link reference validation

        [Fact]
        public void CreateLinkWithNonExistentReference_ShouldThrowException()
        {
            RunTestWithLinks(links =>
            {
                // Act & Assert - should throw exception for referencing non-existent link 10
                var exception = Assert.Throws<InvalidOperationException>(() =>
          {
              ProcessQueryStrict(links, "(() ((1: 10 20)))");
          });

                Assert.Contains("Invalid reference to non-existent link '10'", exception.Message);
                Assert.Contains("--auto-create-missing-references", exception.Message);
            });
        }

        [Fact]
        public void CreateLinkWithValidSelfReference_ShouldSucceed()
        {
            RunTestWithLinks(links =>
            {
                // Act - should succeed because link 1 references itself
                ProcessQueryStrict(links, "(() ((1: 1 1)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Single(allLinks);
                AssertLinkExists(allLinks, 1, 1, 1);
            });
        }

        [Fact]
        public void CreateMultipleLinksWithCrossReferences_ShouldSucceed()
        {
            RunTestWithLinks(links =>
            {
                // Act - should succeed because both links are created in the same operation
                ProcessQueryStrict(links, "(() ((1: 1 2) (2: 2 1)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 1, 2);
                AssertLinkExists(allLinks, 2, 2, 1);
            });
        }

        [Fact]
        public void CreateLinkReferencingExistingLink_ShouldSucceed()
        {
            RunTestWithLinks(links =>
            {
                // Arrange - create first link
                ProcessQueryStrict(links, "(() ((1: 1 1)))");

                // Act - should succeed because link 1 exists
                ProcessQueryStrict(links, "(() ((2: 2 1)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 1, 1);
                AssertLinkExists(allLinks, 2, 2, 1);
            });
        }

        [Fact]
        public void UpdateWithNonExistentReference_ShouldThrowException()
        {
            RunTestWithLinks(links =>
            {
                // Arrange - create initial link
                ProcessQueryStrict(links, "(() ((1: 1 1)))");

                // Act & Assert - should throw exception for referencing non-existent link 99
                var exception = Assert.Throws<InvalidOperationException>(() =>
          {
              ProcessQueryStrict(links, "(((1: 1 1)) ((1: 1 99)))");
          });

                Assert.Contains("Invalid reference to non-existent link '99'", exception.Message);
            });
        }

        [Fact]
        public void CreateNamedLinkWithMissingNamedReferences_ShouldThrowException()
        {
            RunTestWithLinks(links =>
            {
                var exception = Assert.Throws<InvalidOperationException>(() =>
          {
              ProcessQueryStrict(links, "(() ((child: father mother)))");
          });

                Assert.Contains("Invalid reference to non-existent link 'father'", exception.Message);
                Assert.Contains("--auto-create-missing-references", exception.Message);
            });
        }

        [Fact]
        public void CreateLinkWithAutoCreateMissingNumericReferences_ShouldCreatePointLinks()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "(() ((20: 10 20)))");

                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 10, 10, 10);
                AssertLinkExists(allLinks, 20, 10, 20);
            });
        }

        [Fact]
        public void CreateNamedLinkWithAutoCreateMissingNamedReferences_ShouldCreatePointLinks()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "(() ((child: father mother)))");

                var fatherId = links.GetByName("father");
                var motherId = links.GetByName("mother");
                var childId = links.GetByName("child");

                var allLinks = GetAllLinks(links);
                Assert.Equal(3, allLinks.Count);
                AssertLinkExists(allLinks, fatherId, fatherId, fatherId);
                AssertLinkExists(allLinks, motherId, motherId, motherId);
                AssertLinkExists(allLinks, childId, fatherId, motherId);
            });
        }

        [Fact]
        public void CreateLinkWithVariableReferences_ShouldSucceed()
        {
            RunTestWithLinks(links =>
            {
                // Act - should succeed because variables are not validated
                ProcessQueryStrict(links, "(() (($link: $source $target)))");

                // Assert - one link should be created with variables resolved
                var allLinks = GetAllLinks(links);
                Assert.Single(allLinks);
            });
        }

        [Fact]
        public void CreateLinkWithWildcardReferences_ShouldSucceed()
        {
            RunTestWithLinks(links =>
            {
                // Act - should succeed because wildcards are not validated
                ProcessQueryStrict(links, "(() ((1: * *)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Single(allLinks);
            });
        }
    }
}
