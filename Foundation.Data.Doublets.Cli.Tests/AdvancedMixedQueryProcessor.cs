using Platform.Data;
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
    public void Create2LevelNestedLinksTest()
    {
      RunTestWithLinks(links =>
      {
        // Act
        ProcessQuery(links, "(() (((1 1) (2 2))))");

        // Assert
        var allLinks = GetAllLinks(links);
        Assert.Equal(3, allLinks.Count);
        AssertLinkExists(allLinks, 1, 1, 1);
        AssertLinkExists(allLinks, 2, 2, 2);
        AssertLinkExists(allLinks, 3, 1, 2);
      });
    }

    [Fact]
    public void Create3LevelNestedLinksTest()
    {
      RunTestWithLinks(links =>
      {
        // Act
        ProcessQuery(links, "(() (((1 1) ((2 2) (3 3)))))");

        // Assert
        var allLinks = GetAllLinks(links);
        Assert.Equal(5, allLinks.Count);
        AssertLinkExists(allLinks, 1, 1, 1);
        AssertLinkExists(allLinks, 2, 2, 2);
        AssertLinkExists(allLinks, 3, 3, 3);
        AssertLinkExists(allLinks, 4, 2, 3);
        AssertLinkExists(allLinks, 5, 1, 4);
      });
    }

    [Fact]
    public void Create4LevelNestedLinksTest()
    {
      RunTestWithLinks(links =>
      {
        // Act
        ProcessQuery(links, "(() (((1 1) ((2 2) ((3 3) (4 4))))))");

        // Assert
        var allLinks = GetAllLinks(links);
        Assert.Equal(7, allLinks.Count);
        AssertLinkExists(allLinks, 1, 1, 1);
        AssertLinkExists(allLinks, 2, 2, 2);
        AssertLinkExists(allLinks, 3, 3, 3);
        AssertLinkExists(allLinks, 4, 4, 4);
        AssertLinkExists(allLinks, 5, 3, 4);
        AssertLinkExists(allLinks, 6, 2, 5);
        AssertLinkExists(allLinks, 7, 1, 6);
      });
    }

    [Fact]
    public void Create5LevelNestedLinksTest()
    {
      RunTestWithLinks(links =>
      {
        // Structure visualization:
        // Leaves: (1 1), (2 2), (3 3), (4 4), (5 5)
        // ((4 4) (5 5)) => #6: 4->5
        // ((3 3) ((4 4) (5 5))) => #7: 3->6
        // ((2 2) ((3 3) ((4 4) (5 5)))) => #8: 2->7
        // ((1 1) ((2 2) ((3 3) ((4 4) (5 5))))) => #9: 1->8
        //
        // Query: "(() (((1 1) ((2 2) ((3 3) ((4 4) (5 5)))))))"
        ProcessQuery(links, "(() (((1 1) ((2 2) ((3 3) ((4 4) (5 5)))))))");

        var allLinks = GetAllLinks(links);
        Assert.Equal(9, allLinks.Count);

        // Leaf links
        AssertLinkExists(allLinks, 1, 1, 1);
        AssertLinkExists(allLinks, 2, 2, 2);
        AssertLinkExists(allLinks, 3, 3, 3);
        AssertLinkExists(allLinks, 4, 4, 4);
        AssertLinkExists(allLinks, 5, 5, 5);

        // ((4 4) (5 5)) => #6:4->5
        AssertLinkExists(allLinks, 6, 4, 5);

        // ((3 3) ((4 4) (5 5))) => #7:3->6
        AssertLinkExists(allLinks, 7, 3, 6);

        // ((2 2) ((3 3) ((4 4) (5 5)))) => #8:2->7
        AssertLinkExists(allLinks, 8, 2, 7);

        // ((1 1) ((2 2) ((3 3) ((4 4) (5 5))))) => #9:1->8
        AssertLinkExists(allLinks, 9, 1, 8);
      });
    }

    [Fact]
    public void Create6LevelNestedLinksTest()
    {
      RunTestWithLinks(links =>
      {
        // Structure:
        // Leaves: (1 1), (2 2), (3 3), (4 4), (5 5), (6 6)
        // ((5 5) (6 6)) => #7:5->6
        // ((4 4) ((5 5) (6 6))) => #8:4->7
        // ((3 3) ((4 4) ((5 5) (6 6)))) => #9:3->8
        // ((2 2) ((3 3) ((4 4) ((5 5) (6 6))))) => #10:2->9
        // ((1 1) ((2 2) ((3 3) ((4 4) ((5 5) (6 6)))))) => #11:1->10
        //
        // Query: "(() (((1 1) ((2 2) ((3 3) ((4 4) ((5 5) (6 6))))))))"
        ProcessQuery(links, "(() (((1 1) ((2 2) ((3 3) ((4 4) ((5 5) (6 6))))))))");

        var allLinks = GetAllLinks(links);
        Assert.Equal(11, allLinks.Count);

        // Leaf links
        AssertLinkExists(allLinks, 1, 1, 1);
        AssertLinkExists(allLinks, 2, 2, 2);
        AssertLinkExists(allLinks, 3, 3, 3);
        AssertLinkExists(allLinks, 4, 4, 4);
        AssertLinkExists(allLinks, 5, 5, 5);
        AssertLinkExists(allLinks, 6, 6, 6);

        // ((5 5) (6 6)) => #7:5->6
        AssertLinkExists(allLinks, 7, 5, 6);

        // ((4 4) ((5 5) (6 6))) => #8:4->7
        AssertLinkExists(allLinks, 8, 4, 7);

        // ((3 3) ((4 4) ((5 5) (6 6)))) => #9:3->8
        AssertLinkExists(allLinks, 9, 3, 8);

        // ((2 2) ((3 3) ((4 4) ((5 5) (6 6))))) => #10:2->9
        AssertLinkExists(allLinks, 10, 2, 9);

        // ((1 1) ((2 2) ((3 3) ((4 4) ((5 5) (6 6)))))) => #11:1->10
        AssertLinkExists(allLinks, 11, 1, 10);
      });
    }

    [Fact]
    public void Create7LevelNestedLinksTest()
    {
      RunTestWithLinks(links =>
      {
        // Leaves: (1 1), (2 2), (3 3), (4 4), (5 5), (6 6), (7 7)
        // ((6 6) (7 7)) => #8:6->7
        // ((5 5) ((6 6) (7 7))) => #9:5->8
        // ((4 4) ((5 5) ((6 6) (7 7)))) => #10:4->9
        // ((3 3) ((4 4) ((5 5) ((6 6) (7 7))))) => #11:3->10
        // ((2 2) ((3 3) ((4 4) ((5 5) ((6 6) (7 7)))))) => #12:2->11
        // ((1 1) ((2 2) ((3 3) ((4 4) ((5 5) ((6 6) (7 7))))))) => #13:1->12
        //
        // Query: "(() (((1 1) ((2 2) ((3 3) ((4 4) ((5 5) ((6 6) (7 7)))))))))"
        ProcessQuery(links, "(() (((1 1) ((2 2) ((3 3) ((4 4) ((5 5) ((6 6) (7 7)))))))))");

        var allLinks = GetAllLinks(links);
        Assert.Equal(13, allLinks.Count);

        // Leaf links
        AssertLinkExists(allLinks, 1, 1, 1);
        AssertLinkExists(allLinks, 2, 2, 2);
        AssertLinkExists(allLinks, 3, 3, 3);
        AssertLinkExists(allLinks, 4, 4, 4);
        AssertLinkExists(allLinks, 5, 5, 5);
        AssertLinkExists(allLinks, 6, 6, 6);
        AssertLinkExists(allLinks, 7, 7, 7);

        // ((6 6) (7 7)) => #8:6->7
        AssertLinkExists(allLinks, 8, 6, 7);

        // ((5 5) ((6 6) (7 7))) => #9:5->8
        AssertLinkExists(allLinks, 9, 5, 8);

        // ((4 4) ((5 5) ((6 6) (7 7)))) => #10:4->9
        AssertLinkExists(allLinks, 10, 4, 9);

        // ((3 3) ((4 4) ((5 5) ((6 6) (7 7))))) => #11:3->10
        AssertLinkExists(allLinks, 11, 3, 10);

        // ((2 2) ((3 3) ((4 4) ((5 5) ((6 6) (7 7)))))) => #12:2->11
        AssertLinkExists(allLinks, 12, 2, 11);

        // ((1 1) ((2 2) ((3 3) ((4 4) ((5 5) ((6 6) (7 7))))))) => #13:1->12
        AssertLinkExists(allLinks, 13, 1, 12);
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
    public void ExactMatchAndDelete2LevelNestedLinksTest()
    {
      RunTestWithLinks(links =>
      {
        // Arrange
        ProcessQuery(links, "(() (((1 1) (2 2))))");

        // Act
        ProcessQuery(links, "(((3: (1: 1 1) (2: 2 2))) ())");

        // Assert
        var allLinks = GetAllLinks(links);
        Assert.Equal(2, allLinks.Count);
        AssertLinkExists(allLinks, 1, 1, 1);
        AssertLinkExists(allLinks, 2, 2, 2);
      });
    }

    [Fact]
    public void MatchWithExactIndexAndDelete2LevelNestedLinksTest()
    {
      RunTestWithLinks(links =>
      {
        // Arrange
        ProcessQuery(links, "(() (((1 1) (2 2))))");

        // Act
        ProcessQuery(links, "(( (3: (1 *) (* 2)) ) ())");

        // Assert
        var allLinks = GetAllLinks(links);
        Assert.Equal(2, allLinks.Count);
        AssertLinkExists(allLinks, 1, 1, 1);
        AssertLinkExists(allLinks, 2, 2, 2);
      });
    }

    [Fact]
    public void MatchAndDelete2LevelNestedLinksTest()
    {
      RunTestWithLinks(links =>
      {
        // Arrange
        ProcessQuery(links, "(() (((1 1) (2 2))))");

        // Act
        ProcessQuery(links, "(( ((1 *) (* 2)) ) ())");

        // Assert
        var allLinks = GetAllLinks(links);
        Assert.Equal(2, allLinks.Count);
        AssertLinkExists(allLinks, 1, 1, 1);
        AssertLinkExists(allLinks, 2, 2, 2);
      });
    }

    [Fact]
    public void NoExactMatch2LevelNestedLinksTest()
    {
      RunTestWithLinks(links =>
      {
        // Arrange
        ProcessQuery(links, "() ((1: 1 1))");

        // Act
        ProcessQuery(links, "((1: (1: 1 1) (1: 2 1))) ()");

        // Assert
        var allLinks = GetAllLinks(links);
        Assert.Single(allLinks);
        AssertLinkExists(allLinks, 1, 1, 1);
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
    public void SwapEqualSourceAndTargetUsingVariablesHasAllChangesTest()
    {
      RunTestWithLinks(links =>
      {
        // Arrange
        ProcessQuery(links, "() ((1 1) (2 2))");
        ProcessQuery(links, "((1: 1 1)) ((1: 1 2))");

        Options options = new Options();

        var changes = new List<(DoubletLink, DoubletLink)>();
        options.Query = "((($index: $source $target)) (($index: $target $source)))";
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
        AssertLinkExists(allLinks, 1, 2, 1);
        AssertLinkExists(allLinks, 2, 2, 2);
        Assert.Equal(2, changes.Count);
        AssertChangeExists(changes, new DoubletLink(1, 1, 2), new DoubletLink(1, 2, 1));
        AssertChangeExists(changes, new DoubletLink(2, 2, 2), new DoubletLink(2, 2, 2));
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
    public void MakeAllLinksSelfReferencingUsingVariablesTest()
    {
      RunTestWithLinks(links =>
      {
        // Arrange
        ProcessQuery(links, "(() ((1 2) (2 1)))");

        // Act: make all links  self-referencing
        ProcessQuery(links, "((($index: $source $target)) (($index: $index $index)))");

        // Assert
        var allLinks = GetAllLinks(links);
        Assert.Equal(2, allLinks.Count);
        AssertLinkExists(allLinks, 1, 1, 1);
        AssertLinkExists(allLinks, 2, 2, 2);
      });
    }

    [Fact]
    public void MatchSelfReferencingAndMakeThemGoOutFromFirstLinkUsingVariablesTest()
    {
      RunTestWithLinks(links =>
      {
        // Arrange
        ProcessQuery(links, "(() ((1 1) (2 2) (3 1) (4 4)))");

        // Act: match self-referencing links and make them go out from the first link
        ProcessQuery(links, "((($index: $index $index)) (($index: 1 $index)))");

        // Assert
        var allLinks = GetAllLinks(links);
        Assert.Equal(4, allLinks.Count);
        AssertLinkExists(allLinks, 1, 1, 1);
        AssertLinkExists(allLinks, 2, 1, 2);
        AssertLinkExists(allLinks, 3, 3, 1);
        AssertLinkExists(allLinks, 4, 1, 4);
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
        Assert.Single(finalLinks.Where(l => l.Index == finalSonId));
        var finalSonLink = finalLinks.First(l => l.Index == finalSonId);
        Assert.Equal(finalFatherId, finalSonLink.Source);
        Assert.Equal(finalMotherId, finalSonLink.Target);
        Console.WriteLine("[Test] Final state verification completed");
        Console.WriteLine("[Test] UpdateNamedLinkNameTest completed successfully");
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

    // Helper methods
    private static void RunTestWithLinks(Action<NamedLinksDecorator<uint>> testAction, bool enableTracing = false)
    {
      var tempDbFile = Path.GetTempFileName();
      var decorator = new NamedLinksDecorator<uint>(tempDbFile, enableTracing);
      try
      {
        testAction(decorator);
      }
      finally
      {
        if (File.Exists(tempDbFile)) File.Delete(tempDbFile);
        if (File.Exists(decorator.NamedLinksDatabaseFileName)) File.Delete(decorator.NamedLinksDatabaseFileName);
      }
    }

    private static List<DoubletLink> GetAllLinks(NamedLinksDecorator<uint> links)
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