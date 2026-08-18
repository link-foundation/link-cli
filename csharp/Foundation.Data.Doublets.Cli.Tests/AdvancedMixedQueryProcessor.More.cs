// Continuation of the AdvancedMixedQueryProcessor test suite.
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
        public void DeleteAllLinksBySourceAndTargetTest1()
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
        public void NestedDeleteAllLinksBySourceAndTargetTest1()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 2) (2 2)))");

                // Act
                ProcessQuery(links, "((((* *) (* *))) ())");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Empty(allLinks);
            });
        }

        [Fact]
        public void DeleteAllLinksBySourceAndTargetTest2()
        {
            RunTestWithLinks(links =>
            {
                // Arrange
                ProcessQuery(links, "(() ((1 2) (2 1)))");

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

        [Fact]
        public void CreateNamedFamilyLinksTest()
        {
            RunTestWithLinks(links =>
            {
                // Prepare query: create (child: father mother)
                var query = "(() ((child: father mother)))";
                var options = new Options
                {
                    Query = query,
                };
                ProcessQuery(links, options);

                // Assert: links for 'father', 'mother', and 'child' exist and are named
                var fatherId = links.GetByName("father");
                var motherId = links.GetByName("mother");
                var childId = links.GetByName("child");
                Assert.NotEqual(links.Constants.Null, fatherId);
                Assert.NotEqual(links.Constants.Null, motherId);
                Assert.NotEqual(links.Constants.Null, childId);
                Assert.Equal("father", links.GetName(fatherId));
                Assert.Equal("mother", links.GetName(motherId));
                Assert.Equal("child", links.GetName(childId));

                // The child link should have father as source and mother as target
                var allLinks = GetAllLinks(links);
                var childLink = allLinks.First(l => l.Index == childId);
                Assert.Equal(fatherId, childLink.Source);
                Assert.Equal(motherId, childLink.Target);
            });
        }

        [Fact]
        public void CreateTwoNamedLinksTest()
        {
            RunTestWithLinks(links =>
            {
                Console.WriteLine("[Test] Starting UpdateNamedLinkNameTest");

                // Create initial link: (child: father mother)
                Console.WriteLine("[Test] Step 1: Creating initial link");
                var createOptions = new Options { Query = "(() ((child: father mother)))", Trace = true };
                ProcessQuery(links, createOptions);
                Console.WriteLine("[Test] Initial link creation completed");

                // Verify initial state
                Console.WriteLine("[Test] Step 2: Verifying initial state");
                var initialChildId = links.GetByName("child");
                Console.WriteLine($"[Test] Initial child ID: {initialChildId}");
                var initialFatherId = links.GetByName("father");
                Console.WriteLine($"[Test] Initial father ID: {initialFatherId}");
                var initialMotherId = links.GetByName("mother");
                Console.WriteLine($"[Test] Initial mother ID: {initialMotherId}");

                Assert.NotEqual(links.Constants.Null, initialChildId);
                Assert.NotEqual(links.Constants.Null, initialFatherId);
                Assert.NotEqual(links.Constants.Null, initialMotherId);

                var initialLinks = GetAllLinks(links);
                Console.WriteLine($"[Test] Initial links count: {initialLinks.Count}");
                var initialChildLink = initialLinks.First(l => l.Index == initialChildId);
                Assert.Equal(initialFatherId, initialChildLink.Source);
                Assert.Equal(initialMotherId, initialChildLink.Target);
                Console.WriteLine("[Test] Initial state verification completed");

                // Update child link to be named "son" instead
                Console.WriteLine("[Test] Step 3: Updating link name");
                // First, let's try to remove the old name
                Console.WriteLine("[Test] Removing old name 'child'");
                links.RemoveName(initialChildId);
                Console.WriteLine("[Test] Old name removed");

                // Then create the new link with the new name
                Console.WriteLine("[Test] Creating new link with name 'son'");
                var updateOptions = new Options { Query = "(() ((son: father mother)))", Trace = true };
                ProcessQuery(links, updateOptions);
                Console.WriteLine("[Test] New link creation completed");

                // Verify final state
                Console.WriteLine("[Test] Step 4: Verifying final state");
                Assert.Equal(links.Constants.Null, links.GetByName("child"));
                var finalSonId = links.GetByName("son");
                Console.WriteLine($"[Test] Final son ID: {finalSonId}");
                var finalFatherId = links.GetByName("father");
                Console.WriteLine($"[Test] Final father ID: {finalFatherId}");
                var finalMotherId = links.GetByName("mother");
                Console.WriteLine($"[Test] Final mother ID: {finalMotherId}");

                Assert.NotEqual(links.Constants.Null, finalSonId);
                Assert.NotEqual(links.Constants.Null, finalFatherId);
                Assert.NotEqual(links.Constants.Null, finalMotherId);

                var finalLinks = GetAllLinks(links);
                Console.WriteLine($"[Test] Final links count: {finalLinks.Count}");
                var finalSonLink = Assert.Single(finalLinks, l => l.Index == finalSonId);
                Assert.Equal(finalFatherId, finalSonLink.Source);
                Assert.Equal(finalMotherId, finalSonLink.Target);
                Console.WriteLine("[Test] Final state verification completed");
                Console.WriteLine("[Test] UpdateNamedLinkNameTest completed successfully");
            }, enableTracing: true);
        }

        [Fact]
        public void UpdateNamedLinkNameTest()
        {
            Console.WriteLine("[Test] ===== Starting UpdateNamedLinkNameTest =====");
            RunTestWithLinks(links =>
            {
                try
                {
                    Console.WriteLine($"[Test] Constants: Null={links.Constants.Null}, Any={links.Constants.Any}, Continue={links.Constants.Continue}");
                    // Step 1: Creating initial link
                    Console.WriteLine("[Test] Step 1: Creating initial link");
                    var createQuery = "(() ((child: father mother)))";
                    Console.WriteLine($"[Test] Query: {createQuery}");

                    var createOptions = new Options
                    {
                        Query = createQuery,
                        Trace = true
                    };
                    ProcessQuery(links, createOptions);
                    Console.WriteLine("[Test] Initial link creation completed");

                    // Step 2: Verify initial state
                    Console.WriteLine("[Test] Step 2: Verifying initial state");
                    var childId = links.GetByName("child");
                    Console.WriteLine($"[Test] Initial child ID: {childId}");
                    var fatherId = links.GetByName("father");
                    Console.WriteLine($"[Test] Initial father ID: {fatherId}");
                    var motherId = links.GetByName("mother");
                    Console.WriteLine($"[Test] Initial mother ID: {motherId}");

                    var initialLinks = links.All().ToList();
                    Console.WriteLine($"[Test] Initial links count: {initialLinks.Count}");
                    foreach (var link in initialLinks)
                    {
                        var source = links.GetSource(link);
                        var target = links.GetTarget(link);
                        Console.WriteLine($"[Test] Initial link: Index={link}, Source={source}, Target={target}");
                    }
                    Console.WriteLine("[Test] Initial state verification completed");

                    // Step 3: Update link name
                    Console.WriteLine("[Test] Step 3: Updating link name from 'child' to 'son'");
                    var updateQuery = "(((child: father mother)) ((son: father mother)))";
                    Console.WriteLine($"[Test] Query: {updateQuery}");

                    // Log state before update
                    Console.WriteLine("[Test] Current state before update:");
                    Console.WriteLine($"[Test] - child name exists: {links.GetByName("child") != 0}");
                    Console.WriteLine($"[Test] - son name exists: {links.GetByName("son") != 0}");
                    Console.WriteLine($"[Test] - father name exists: {links.GetByName("father") != 0}");
                    Console.WriteLine($"[Test] - mother name exists: {links.GetByName("mother") != 0}");

                    Console.WriteLine("[Test] Starting ProcessQuery for update...");
                    Console.WriteLine("[Test] Current links before update:");
                    foreach (var link in links.All())
                    {
                        var source = links.GetSource(link);
                        var target = links.GetTarget(link);
                        Console.WriteLine($"[Test]   Link: Index={link}, Source={source}, Target={target}");
                    }

                    // Add detailed tracing for the update operation
                    var updateOptions = new Options
                    {
                        Query = updateQuery,
                        Trace = true,
                        ChangesHandler = (before, after) =>
                  {
                      Console.WriteLine($"[Test] Update ChangesHandler called:");
                      Console.WriteLine($"[Test] - Before state: {before}");
                      Console.WriteLine($"[Test] - After state: {after}");

                      // Log name states during change
                      Console.WriteLine($"[Test] - child name during change: {links.GetByName("child")}");
                      Console.WriteLine($"[Test] - son name during change: {links.GetByName("son")}");
                      Console.WriteLine($"[Test] - father name during change: {links.GetByName("father")}");
                      Console.WriteLine($"[Test] - mother name during change: {links.GetByName("mother")}");

                      // Log all links during change
                      Console.WriteLine("[Test] - All links during change:");
                      foreach (var link in links.All())
                      {
                          var source = links.GetSource(link);
                          var target = links.GetTarget(link);
                          Console.WriteLine($"[Test]   Link: Index={link}, Source={source}, Target={target}");
                      }

                      // Add detailed tracing for link creation
                      if (after != null && before == null)
                      {
                          var afterLink = new DoubletLink(after);
                          var source = links.GetSource(after);
                          var target = links.GetTarget(after);
                          Console.WriteLine($"[Test] Creating new link: Index={afterLink.Index}, Source={source}, Target={target}");
                          Console.WriteLine($"[Test] Checking if link exists: {links.Exists<uint, LinksConstants<uint>>(afterLink.Index)}");
                          Console.WriteLine($"[Test] Checking if source exists: {links.Exists<uint, LinksConstants<uint>>(source)}");
                          Console.WriteLine($"[Test] Checking if target exists: {links.Exists<uint, LinksConstants<uint>>(target)}");

                          // Log all names before creation
                          Console.WriteLine("[Test] Names before creation:");
                          foreach (var name in new[] { "child", "son", "father", "mother" })
                          {
                              var id = links.GetByName(name);
                              Console.WriteLine($"[Test] - {name}: {id}");
                          }
                      }

                      return links.Constants.Continue;
                  }
                    };

                    ProcessQuery(links, updateOptions);
                    Console.WriteLine("[Test] Update operation completed");

                    // Step 4: Verify final state
                    Console.WriteLine("[Test] Step 4: Verifying final state");
                    var finalChildId = links.GetByName("child");
                    Console.WriteLine($"[Test] Final child ID: {finalChildId}");
                    var finalSonId = links.GetByName("son");
                    Console.WriteLine($"[Test] Final son ID: {finalSonId}");
                    var finalFatherId = links.GetByName("father");
                    Console.WriteLine($"[Test] Final father ID: {finalFatherId}");
                    var finalMotherId = links.GetByName("mother");
                    Console.WriteLine($"[Test] Final mother ID: {finalMotherId}");

                    var finalLinks = links.All().ToList();
                    Console.WriteLine($"[Test] Final links count: {finalLinks.Count}");
                    foreach (var link in finalLinks)
                    {
                        var source = links.GetSource(link);
                        var target = links.GetTarget(link);
                        Console.WriteLine($"[Test] Final link: Index={link}, Source={source}, Target={target}");
                    }

                    // Verify the update was successful
                    Assert.Equal<uint>(0, finalChildId); // Old name should be gone
                    Assert.NotEqual<uint>(0, finalSonId); // New name should exist
                    Assert.Equal<uint>(finalFatherId, links.GetSource(finalSonId)); // Source should be father
                    Assert.Equal<uint>(finalMotherId, links.GetTarget(finalSonId)); // Target should be mother

                    Console.WriteLine("[Test] ===== UpdateNamedLinkNameTest completed successfully =====");
                }
                catch (Exception ex)
                {
                    Console.WriteLine($"[Test] Error in UpdateNamedLinkNameTest: {ex}");
                    Console.WriteLine($"[Test] Stack trace: {ex.StackTrace}");
                    throw;
                }
            }, enableTracing: true);
        }

        [Fact]
        public void DeleteNamedFamilyLinksRemovesNamesTest()
        {
            RunTestWithLinks(links =>
            {
                // Prepare query: create (child: father mother)
                var query = "(() ((child: father mother)))";
                var options = new Options
                {
                    Query = query,
                };
                ProcessQuery(links, options);

                // Delete the 'child' link
                var childId = links.GetByName("child");
                links.Delete(childId);

                // Assert: 'child' name is removed, 'father' and 'mother' remain
                Assert.Equal(links.Constants.Null, links.GetByName("child"));
                Assert.NotEqual(links.Constants.Null, links.GetByName("father"));
                Assert.NotEqual(links.Constants.Null, links.GetByName("mother"));
            });
        }

        [Fact]
        public void DeleteNamedLinkTest()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "(() ((child: father mother)))");

                ProcessQuery(links, "(((*:)) ())");

                Assert.Equal(links.Constants.Null, links.GetByName("child"));
                Assert.Equal(links.Constants.Null, links.GetByName("father"));
                Assert.Equal(links.Constants.Null, links.GetByName("mother"));
            });
        }

        [Fact]
        public void DeleteByNamesTest()
        {
            RunTestWithLinks(links =>
            {
                // Create link by name
                ProcessQuery(links, "(() ((child: father mother)))");

                // Delete link by name
                ProcessQuery(links, "(((child: father mother)) ())");

                Assert.Equal(links.Constants.Null, links.GetByName("child"));
                Assert.NotEqual(links.Constants.Null, links.GetByName("father"));
                Assert.NotEqual(links.Constants.Null, links.GetByName("mother"));
            });
        }

        [Fact]
        public void NameLookupConsistencyTest()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "(() ((x: 1 2)))");
                ProcessQuery(links, "(((x: 1 2)) ((y: 1 2)))");
                ProcessQuery(links, "(((y: 1 2)) ((z: 1 2)))");
                links.Delete(links.GetByName("z"));
                Assert.Equal(links.Constants.Null, links.GetByName("x"));
                Assert.Equal(links.Constants.Null, links.GetByName("y"));
                Assert.Equal(links.Constants.Null, links.GetByName("z"));
            });
        }

        [Fact]
        public void CreateNamedLinkWithStringId_ShouldCreateSingleLink()
        {
            RunTestWithLinks(links =>
            {
                var options = new Options { Query = "(() ((link: link link)))", Trace = true };
                ProcessQuery(links, options);
                var allLinks = GetAllLinks(links);
                // This should only create a single named link with string id 'link'
                Assert.Single(allLinks);
                var linkId = links.GetByName("link");
                Assert.NotEqual(links.Constants.Null, linkId);
                var link = allLinks.First();
                Assert.Equal(linkId, link.Index);
                Assert.Equal(linkId, link.Source);
                Assert.Equal(linkId, link.Target);
                Assert.Equal("link", links.GetName(linkId));
            }, enableTracing: true);
        }

        [Fact]
        public void CreateLinkWithIntegerId_ShouldCreateSingleLink()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "(() ((1: 1 1)))");
                var allLinks = GetAllLinks(links);
                Assert.Single(allLinks);
                var link = allLinks.First();
                Assert.Equal(1u, link.Index);
                Assert.Equal(1u, link.Source);
                Assert.Equal(1u, link.Target);
            });
        }

        [Fact]
        public void CreateLeftCompositeIntegerChildrenWithoutExtraLeaf_ShouldSucceed()
        {
            RunTestWithLinks(links =>
            {
                // Act
                ProcessQuery(links, "(() ((1: 1 1)))");
                ProcessQuery(links, "(() ((2: 2 1)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 1, 1);
                AssertLinkExists(allLinks, 2, 2, 1);
            });
        }

        [Fact]
        public void CreateRightCompositeIntegerChildrenWithoutExtraLeaf_ShouldSucceed()
        {
            RunTestWithLinks(links =>
            {
                // Act
                ProcessQuery(links, "(() ((1: 1 1)))");
                ProcessQuery(links, "(() ((2: 1 2)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                AssertLinkExists(allLinks, 1, 1, 1);
                AssertLinkExists(allLinks, 2, 1, 2);
            });
        }

        [Fact]
        public void CreateLeftCompositeStringChildrenWithoutExtraLeaf_ShouldSucceed()
        {
            RunTestWithLinks(links =>
            {
                // Act
                ProcessQuery(links, "(() ((type: type type)))");
                ProcessQuery(links, "(() ((link: link type)))");

                // Assert
                var allLinks = GetAllLinks(links);
                // Expect only two links, but extra self-referential named link for 'link' is created indicating a bug.
                Assert.Equal(2, allLinks.Count);
                var typeId = links.GetByName("type");
                var linkId = links.GetByName("link");
                AssertLinkExists(allLinks, typeId, typeId, typeId);
                AssertLinkExists(allLinks, linkId, linkId, typeId);
            });
        }

        [Fact]
        public void CreateRightCompositeStringChildrenWithoutExtraLeaf_ShouldSucceed()
        {
            RunTestWithLinks(links =>
            {
                // Act
                ProcessQuery(links, "(() ((type: type type)))");
                ProcessQuery(links, "(() ((link: type link)))");

                // Assert
                var allLinks = GetAllLinks(links);
                Assert.Equal(2, allLinks.Count);
                var typeId = links.GetByName("type");
                var linkId = links.GetByName("link");
                AssertLinkExists(allLinks, typeId, typeId, typeId);
                AssertLinkExists(allLinks, linkId, typeId, linkId);
            });
        }

        // ============================================
        // Link Deduplication Tests
        // ============================================

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

        [Fact]
        public void StringAliasesInVariableRestriction_ShouldConstrainMatchesToNamedLinks()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "(() ((father: father father)))");
                ProcessQuery(links, "(() ((mother: mother mother)))");
                ProcessQuery(links, "(() ((child: father mother)))");

                var fatherId = links.GetByName("father");
                var motherId = links.GetByName("mother");
                var childId = links.GetByName("child");

                ProcessQuery(links, "((($id: father mother)) (($id: mother father)))");

                var allLinks = GetAllLinks(links);
                Assert.Equal(3, allLinks.Count);
                AssertLinkExists(allLinks, fatherId, fatherId, fatherId);
                AssertLinkExists(allLinks, motherId, motherId, motherId);
                AssertLinkExists(allLinks, childId, motherId, fatherId);
            });
        }

        [Fact]
        public void Issue20_SubstituteMatchedLinkAndOutgoingLink_ShouldPreserveExistingParts()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "(() ((1: 1 1) (18: 1 21) (19: 1 20) (20: 20 20) (21: 21 21)))");

                ProcessQuery(links, "((($i: 1 21)) (($i: $s $t) ($i 20)))");

                var allLinks = GetAllLinks(links);
                Assert.Equal(6, allLinks.Count);
                AssertLinkExists(allLinks, 1, 1, 1);
                AssertLinkExists(allLinks, 18, 1, 21);
                AssertLinkExists(allLinks, 19, 1, 20);
                AssertLinkExists(allLinks, 20, 20, 20);
                AssertLinkExists(allLinks, 21, 21, 21);

                var outgoingLink = Assert.Single(allLinks, link => link.Source == 18 && link.Target == 20);
                Assert.NotEqual(links.Constants.Null, outgoingLink.Index);
                Assert.NotEqual(links.Constants.Any, outgoingLink.Index);
                Assert.DoesNotContain(allLinks, link => link.Index == links.Constants.Any || link.Source == links.Constants.Any || link.Target == links.Constants.Any);
            });
        }

        [Fact]
        public void Issue20_SubstituteFullPointWithUnboundParts_ShouldKeepFullPoint()
        {
            RunTestWithLinks(links =>
            {
                ProcessQuery(links, "(() ((21: 21 21)))");

                ProcessQuery(links, "(((21: 21 21)) ((21: $s $t)))");

                var allLinks = GetAllLinks(links);
                Assert.Single(allLinks);
                AssertLinkExists(allLinks, 21, 21, 21);
                Assert.DoesNotContain(allLinks, link => link.Source == links.Constants.Any || link.Target == links.Constants.Any);
            });
        }

        [Fact]
        public void EnsureCreated_WithSpecialAnyReference_ShouldThrowControlledException()
        {
            RunTestWithLinks(links =>
            {
                var exception = Assert.Throws<InvalidOperationException>(() => LinksExtensions.EnsureCreated(links, links.Constants.Any));

                Assert.Contains("unsupported link address", exception.Message);
            });
        }

        // Helper methods

        /// <summary>
        /// Wall-clock budget for a single test body. Every test here is expected to finish in
        /// milliseconds, so this is a deadlock guard, not a performance assertion: the previous value of
        /// one second was tight enough that a loaded CI runner failed tests that were perfectly correct.
        /// Override with the LINK_CLI_TEST_TIMEOUT_SECONDS environment variable.
    }
}
