using Platform.Data.Doublets;

using static Foundation.Data.Doublets.Cli.ChangesSimplifier;

namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
  public class ChangesSimplifierTests
  {
    [Fact]
    public void SimplifyChanges_SpecificExample_RemovesIntermediateStates()
    {
      // Arrange
      var changes = new List<(Link<uint> Before, Link<uint> After)>
      {
        // (1: 2 1) ↦ (1: 0 0)
        (new Link<uint>(index: 1, source: 2, target: 1), new Link<uint>(index: 1, source: 0, target: 0)),
        
        // (2: 1 2) ↦ (2: 0 0)
        (new Link<uint>(index: 2, source: 1, target: 2), new Link<uint>(index: 2, source: 0, target: 0)),
        
        // (2: 0 0) ↦ (0: 0 0)
        (new Link<uint>(index: 2, source: 0, target: 0), new Link<uint>(index: 0, source: 0, target: 0)),
        
        // (1: 0 0) ↦ (0: 0 0)
        (new Link<uint>(index: 1, source: 0, target: 0), new Link<uint>(index: 0, source: 0, target: 0))
      };

      // Expected simplified changes:
      // (1: 2 1) ↦ (0: 0 0)
      // (2: 1 2) ↦ (0: 0 0)
      var expectedSimplifiedChanges = new List<(Link<uint> Before, Link<uint> After)>
      {
        (new Link<uint>(index: 1, source: 2, target: 1), new Link<uint>(index: 0, source: 0, target: 0)),
        (new Link<uint>(index: 2, source: 1, target: 2), new Link<uint>(index: 0, source: 0, target: 0))
      };

      // Act
      var simplifiedChanges = SimplifyChanges(changes).ToList();

      // Assert
      Assert.Equal(expectedSimplifiedChanges.Count, simplifiedChanges.Count);

      foreach (var expected in expectedSimplifiedChanges)
      {
        Assert.Contains(simplifiedChanges, actual =>
            actual.Before.Index == expected.Before.Index &&
            actual.Before.Source == expected.Before.Source &&
            actual.Before.Target == expected.Before.Target &&
            actual.After.Index == expected.After.Index &&
            actual.After.Source == expected.After.Source &&
            actual.After.Target == expected.After.Target
        );
      }
    }

    [Fact]
    public void SimplifyChanges_MultipleChainsFromSameBefore_RemovesIntermediateStates()
    {
      // Arrange
      var changes = new List<(Link<uint> Before, Link<uint> After)>
      {
        // (0: 0 0) ↦ (1: 0 0)
        (new Link<uint>(index: 0, source: 0, target: 0), new Link<uint>(index: 1, source: 0, target: 0)),
        
        // (1: 0 0) ↦ (1: 1 2)
        (new Link<uint>(index: 1, source: 0, target: 0), new Link<uint>(index: 1, source: 1, target: 2)),
        
        // (0: 0 0) ↦ (2: 0 0)
        (new Link<uint>(index: 0, source: 0, target: 0), new Link<uint>(index: 2, source: 0, target: 0)),
        
        // (2: 0 0) ↦ (2: 2 1)
        (new Link<uint>(index: 2, source: 0, target: 0), new Link<uint>(index: 2, source: 2, target: 1))
      };

      // Expected simplified changes:
      // (0: 0 0) ↦ (1: 1 2)
      // (0: 0 0) ↦ (2: 2 1)
      var expectedSimplifiedChanges = new List<(Link<uint> Before, Link<uint> After)>
      {
        (new Link<uint>(index: 0, source: 0, target: 0), new Link<uint>(index: 1, source: 1, target: 2)),
        (new Link<uint>(index: 0, source: 0, target: 0), new Link<uint>(index: 2, source: 2, target: 1))
      };

      // Act
      var simplifiedChanges = SimplifyChanges(changes).ToList();

      // Assert
      Assert.Equal(expectedSimplifiedChanges.Count, simplifiedChanges.Count);

      foreach (var expected in expectedSimplifiedChanges)
      {
        Assert.Contains(simplifiedChanges, actual =>
            actual.Before.Index == expected.Before.Index &&
            actual.Before.Source == expected.Before.Source &&
            actual.Before.Target == expected.Before.Target &&
            actual.After.Index == expected.After.Index &&
            actual.After.Source == expected.After.Source &&
            actual.After.Target == expected.After.Target
        );
      }
    }

    [Fact]
    public void SimplifyChanges_NoChange_StillKeepsFirstAndLastState()
    {
      // Arrange
      // These changes represent a "read" operation: the link is read but not changed.
      // (1: 2 1) -> (1: 2 1)
      // (2: 1 2) -> (2: 1 2)
      var changes = new List<(Link<uint> Before, Link<uint> After)>
      {
        (new Link<uint>(index: 1, source: 2, target: 1), new Link<uint>(index: 1, source: 2, target: 1)),
        (new Link<uint>(index: 2, source: 1, target: 2), new Link<uint>(index: 2, source: 1, target: 2))
      };

      // Expected simplified changes:
      // They are the same because no actual changes occurred, but we still keep them.
      var expectedSimplifiedChanges = new List<(Link<uint> Before, Link<uint> After)>
      {
        (new Link<uint>(index: 1, source: 2, target: 1), new Link<uint>(index: 1, source: 2, target: 1)),
        (new Link<uint>(index: 2, source: 1, target: 2), new Link<uint>(index: 2, source: 1, target: 2))
      };

      // Act
      var simplifiedChanges = SimplifyChanges(changes).ToList();

      // Assert
      Assert.Equal(expectedSimplifiedChanges.Count, simplifiedChanges.Count);
      foreach (var expected in expectedSimplifiedChanges)
      {
        Assert.Contains(simplifiedChanges, actual =>
            actual.Before.Index == expected.Before.Index &&
            actual.Before.Source == expected.Before.Source &&
            actual.Before.Target == expected.Before.Target &&
            actual.After.Index == expected.After.Index &&
            actual.After.Source == expected.After.Source &&
            actual.After.Target == expected.After.Target
        );
      }
    }

    [Fact]
    public void SimplifyChanges_MultipleBranchesFromSameInitial_ProducesCorrectFinalStates()
    {
      // Arrange
      // These transitions demonstrate several branches starting from (0: 0 0),
      // each ultimately reaching (1: 1 1), (2: 2 2), (3: 3 3), or (4: 4 4).
      // The intermediate states (1: 0 0), (2: 0 0), (3: 0 0), (4: 0 0) are
      // transitions on the way to the final states.
      //
      // Original transitions (Before -> After):
      // (0: 0 0) -> (1: 0 0)
      // (1: 0 0) -> (1: 1 1)
      // (0: 0 0) -> (2: 0 0)
      // (2: 0 0) -> (2: 2 2)
      // (0: 0 0) -> (3: 0 0)
      // (3: 0 0) -> (3: 3 3)
      // (0: 0 0) -> (4: 0 0)
      // (4: 0 0) -> (4: 4 4)
      var changes = new List<(Link<uint> Before, Link<uint> After)>
    {
        (new Link<uint>(0, 0, 0), new Link<uint>(1, 0, 0)),
        (new Link<uint>(1, 0, 0), new Link<uint>(1, 1, 1)),
        (new Link<uint>(0, 0, 0), new Link<uint>(2, 0, 0)),
        (new Link<uint>(2, 0, 0), new Link<uint>(2, 2, 2)),
        (new Link<uint>(0, 0, 0), new Link<uint>(3, 0, 0)),
        (new Link<uint>(3, 0, 0), new Link<uint>(3, 3, 3)),
        (new Link<uint>(0, 0, 0), new Link<uint>(4, 0, 0)),
        (new Link<uint>(4, 0, 0), new Link<uint>(4, 4, 4))
    };

      // Expected final transitions (After simplification):
      // (0: 0 0) -> (1: 1 1)
      // (0: 0 0) -> (2: 2 2)
      // (0: 0 0) -> (3: 3 3)
      // (0: 0 0) -> (4: 4 4)
      var expectedSimplifiedChanges = new List<(Link<uint> Before, Link<uint> After)>
      {
          (new Link<uint>(0, 0, 0), new Link<uint>(1, 1, 1)),
          (new Link<uint>(0, 0, 0), new Link<uint>(2, 2, 2)),
          (new Link<uint>(0, 0, 0), new Link<uint>(3, 3, 3)),
          (new Link<uint>(0, 0, 0), new Link<uint>(4, 4, 4))
      };

      // Act
      var simplifiedChanges = SimplifyChanges(changes).ToList();

      // Assert
      // We expect exactly four final states from (0: 0 0), each linking directly
      // to one of the final links (1: 1 1), (2: 2 2), (3: 3 3), or (4: 4 4).
      Assert.Equal(expectedSimplifiedChanges.Count, simplifiedChanges.Count);

      // Check that each expected (Before, After) pair is present in the result.
      foreach (var expected in expectedSimplifiedChanges)
      {
        Assert.Contains(simplifiedChanges, actual =>
            actual.Before.Index == expected.Before.Index &&
            actual.Before.Source == expected.Before.Source &&
            actual.Before.Target == expected.Before.Target &&
            actual.After.Index == expected.After.Index &&
            actual.After.Source == expected.After.Source &&
            actual.After.Target == expected.After.Target
        );
      }
    }

    [Fact]
    public void SimplifyChanges_MultipleBranchesFromSameInitial_ProducesCorrectFinalStates_InCorrectOrder()
    {
      // Arrange
      // Original transitions (Before -> After):
      // (0: 0 0) -> (1: 0 0)
      // (1: 0 0) -> (1: 1 1)
      // (0: 0 0) -> (2: 0 0)
      // (2: 0 0) -> (2: 2 2)
      // (0: 0 0) -> (3: 0 0)
      // (3: 0 0) -> (3: 3 3)
      // (0: 0 0) -> (4: 0 0)
      // (4: 0 0) -> (4: 4 4)
      var changes = new List<(Link<uint> Before, Link<uint> After)>
    {
        (new Link<uint>(0, 0, 0), new Link<uint>(1, 0, 0)),
        (new Link<uint>(1, 0, 0), new Link<uint>(1, 1, 1)),
        (new Link<uint>(0, 0, 0), new Link<uint>(2, 0, 0)),
        (new Link<uint>(2, 0, 0), new Link<uint>(2, 2, 2)),
        (new Link<uint>(0, 0, 0), new Link<uint>(3, 0, 0)),
        (new Link<uint>(3, 0, 0), new Link<uint>(3, 3, 3)),
        (new Link<uint>(0, 0, 0), new Link<uint>(4, 0, 0)),
        (new Link<uint>(4, 0, 0), new Link<uint>(4, 4, 4))
    };

      // Expected final transitions (After simplification):
      // (0: 0 0) -> (1: 1 1)
      // (0: 0 0) -> (2: 2 2)
      // (0: 0 0) -> (3: 3 3)
      // (0: 0 0) -> (4: 4 4)

      // Act
      var simplifiedChanges = SimplifyChanges(changes).ToList();

      // Assert
      // 1) Ensure we got exactly 4 transitions after simplification.
      Assert.Equal(4, simplifiedChanges.Count);

      // 2) Check that each final pair is present (as before),
      //    AND that they appear in ascending order:
      //    first => After.Index == 1,
      //    second => After.Index == 2,
      //    third => After.Index == 3,
      //    fourth => After.Index == 4.

      Assert.Collection(
          simplifiedChanges,
          // 1st final link: (0:0 0) -> (1:1 1)
          first =>
          {
            Assert.Equal(0u, first.Before.Index);
            Assert.Equal(0u, first.Before.Source);
            Assert.Equal(0u, first.Before.Target);

            Assert.Equal(1u, first.After.Index);
            Assert.Equal(1u, first.After.Source);
            Assert.Equal(1u, first.After.Target);
          },
          // 2nd final link: (0:0 0) -> (2:2 2)
          second =>
          {
            Assert.Equal(0u, second.Before.Index);
            Assert.Equal(0u, second.Before.Source);
            Assert.Equal(0u, second.Before.Target);

            Assert.Equal(2u, second.After.Index);
            Assert.Equal(2u, second.After.Source);
            Assert.Equal(2u, second.After.Target);
          },
          // 3rd final link: (0:0 0) -> (3:3 3)
          third =>
          {
            Assert.Equal(0u, third.Before.Index);
            Assert.Equal(0u, third.Before.Source);
            Assert.Equal(0u, third.Before.Target);

            Assert.Equal(3u, third.After.Index);
            Assert.Equal(3u, third.After.Source);
            Assert.Equal(3u, third.After.Target);
          },
          // 4th final link: (0:0 0) -> (4:4 4)
          fourth =>
          {
            Assert.Equal(0u, fourth.Before.Index);
            Assert.Equal(0u, fourth.Before.Source);
            Assert.Equal(0u, fourth.Before.Target);

            Assert.Equal(4u, fourth.After.Index);
            Assert.Equal(4u, fourth.After.Source);
            Assert.Equal(4u, fourth.After.Target);
          }
      );
    }


    // [Fact]
    // public void SimplifyChanges_NoChanges_ReturnsEmpty()
    // {
    //     // Arrange
    //     var changes = new List<(Link<uint> Before, Link<uint> After)>();

    //     // Act
    //     var simplified = ChangesSimplifier.SimplifyChanges(changes);

    //     // Assert
    //     Assert.Empty(simplified);
    // }

    // [Fact]
    // public void SimplifyChanges_SingleChange_ReturnsSameChange()
    // {
    //     // Arrange
    //     var changes = new List<(Link<uint> Before, Link<uint> After)>
    //     {
    //         (new Link<uint>(1, 2, 3), new Link<uint>(1, 4, 5))
    //     };

    //     // Act
    //     var simplified = ChangesSimplifier.SimplifyChanges(changes).ToList();

    //     // Assert
    //     Assert.Single(simplified);
    //     Assert.Equal(1u, simplified[0].Before.Index);
    //     Assert.Equal(0u, simplified[0].Before.Source); // Assuming default values for other fields
    //     Assert.Equal(0u, simplified[0].Before.Target);
    //     Assert.Equal(new Link<uint>(1, 4, 5), simplified[0].After);
    // }

    // [Fact]
    // public void SimplifyChanges_MultipleNonOverlappingChanges_ReturnsAllChanges()
    // {
    //     // Arrange
    //     var changes = new List<(Link<uint> Before, Link<uint> After)>
    //     {
    //         (new Link<uint>(1, 2, 3), new Link<uint>(1, 4, 5)),
    //         (new Link<uint>(2, 3, 4), new Link<uint>(2, 5, 6)),
    //         (new Link<uint>(3, 4, 5), new Link<uint>(3, 6, 7))
    //     };

    //     // Act
    //     var simplified = ChangesSimplifier.SimplifyChanges(changes).ToList();

    //     // Assert
    //     Assert.Equal(3, simplified.Count);

    //     Assert.Contains(simplified, c => c.Before.Index == 1 && c.After.Index == 1 && c.After.Source == 4 && c.After.Target == 5);
    //     Assert.Contains(simplified, c => c.Before.Index == 2 && c.After.Index == 2 && c.After.Source == 5 && c.After.Target == 6);
    //     Assert.Contains(simplified, c => c.Before.Index == 3 && c.After.Index == 3 && c.After.Source == 6 && c.After.Target == 7);
    // }

    // [Fact]
    // public void SimplifyChanges_MultipleOverlappingChanges_ReturnsInitialToFinal()
    // {
    //     // Arrange
    //     var changes = new List<(Link<uint> Before, Link<uint> After)>
    //     {
    //         (new Link<uint>(1, 2, 3), new Link<uint>(1, 4, 5)),
    //         (new Link<uint>(1, 4, 5), new Link<uint>(1, 6, 7)),
    //         (new Link<uint>(2, 3, 4), new Link<uint>(2, 5, 6)),
    //         (new Link<uint>(2, 5, 6), new Link<uint>(2, 0, 0)),
    //         (new Link<uint>(3, 4, 5), new Link<uint>(3, 6, 7))
    //     };

    //     // Act
    //     var simplified = ChangesSimplifier.SimplifyChanges(changes).ToList();

    //     // Assert
    //     Assert.Equal(3, simplified.Count);

    //     // Link 1: from (1,2,3) to (1,6,7)
    //     var link1 = simplified.FirstOrDefault(c => c.Before.Index == 1);
    //     Assert.Equal(new Link<uint>(1, 2, 3), link1.Before);
    //     Assert.Equal(new Link<uint>(1, 6, 7), link1.After);

    //     // Link 2: from (2,3,4) to (2,0,0)
    //     var link2 = simplified.FirstOrDefault(c => c.Before.Index == 2);
    //     Assert.Equal(new Link<uint>(2, 3, 4), link2.Before);
    //     Assert.Equal(new Link<uint>(2, 0, 0), link2.After);

    //     // Link 3: from (3,4,5) to (3,6,7)
    //     var link3 = simplified.FirstOrDefault(c => c.Before.Index == 3);
    //     Assert.Equal(new Link<uint>(3, 4, 5), link3.Before);
    //     Assert.Equal(new Link<uint>(3, 6, 7), link3.After);
    // }

    // [Fact]
    // public void SimplifyChanges_ComplexScenario_MixedChanges()
    // {
    //     // Arrange
    //     var changes = new List<(Link<uint> Before, Link<uint> After)>
    //     {
    //         // Link 1: Multiple changes
    //         (new Link<uint>(1, 2, 3), new Link<uint>(1, 4, 5)),
    //         (new Link<uint>(1, 4, 5), new Link<uint>(1, 6, 7)),
    //         (new Link<uint>(1, 6, 7), new Link<uint>(1, 8, 9)),

    //         // Link 2: Single change
    //         (new Link<uint>(2, 3, 4), new Link<uint>(2, 5, 6)),

    //         // Link 3: Multiple changes
    //         (new Link<uint>(3, 4, 5), new Link<uint>(3, 6, 7)),
    //         (new Link<uint>(3, 6, 7), new Link<uint>(3, 0, 0)),

    //         // Link 4: No changes
    //     };

    //     // Act
    //     var simplified = ChangesSimplifier.SimplifyChanges(changes).ToList();

    //     // Assert
    //     Assert.Equal(3, simplified.Count);

    //     // Link 1: from (1,2,3) to (1,8,9)
    //     var link1 = simplified.FirstOrDefault(c => c.Before.Index == 1);
    //     Assert.Equal(new Link<uint>(1, 2, 3), link1.Before);
    //     Assert.Equal(new Link<uint>(1, 8, 9), link1.After);

    //     // Link 2: from (2,3,4) to (2,5,6)
    //     var link2 = simplified.FirstOrDefault(c => c.Before.Index == 2);
    //     Assert.Equal(new Link<uint>(2, 3, 4), link2.Before);
    //     Assert.Equal(new Link<uint>(2, 5, 6), link2.After);

    //     // Link 3: from (3,4,5) to (3,0,0)
    //     var link3 = simplified.FirstOrDefault(c => c.Before.Index == 3);
    //     Assert.Equal(new Link<uint>(3, 4, 5), link3.Before);
    //     Assert.Equal(new Link<uint>(3, 0, 0), link3.After);
    // }

    // [Fact]
    // public void SimplifyChanges_SameAfterMultipleChanges_ReturnsLastChange()
    // {
    //     // Arrange
    //     var changes = new List<(Link<uint> Before, Link<uint> After)>
    //     {
    //         (new Link<uint>(1, 2, 3), new Link<uint>(1, 4, 5)),
    //         (new Link<uint>(1, 4, 5), new Link<uint>(1, 6, 7)),
    //         (new Link<uint>(1, 6, 7), new Link<uint>(1, 4, 5)) // Reverting back
    //     };

    //     // Act
    //     var simplified = ChangesSimplifier.SimplifyChanges(changes).ToList();

    //     // Assert
    //     Assert.Single(simplified);

    //     // Link 1: from (1,2,3) to (1,4,5)
    //     var link1 = simplified.First();
    //     Assert.Equal(new Link<uint>(1, 2, 3), link1.Before);
    //     Assert.Equal(new Link<uint>(1, 4, 5), link1.After);
    // }

    // [Fact]
    // public void SimplifyChanges_DuplicateChanges_IgnoresDuplicates()
    // {
    //     // Arrange
    //     var changes = new List<(Link<uint> Before, Link<uint> After)>
    //     {
    //         (new Link<uint>(1, 2, 3), new Link<uint>(1, 4, 5)),
    //         (new Link<uint>(1, 2, 3), new Link<uint>(1, 4, 5)), // Duplicate
    //         (new Link<uint>(2, 3, 4), new Link<uint>(2, 5, 6)),
    //         (new Link<uint>(2, 3, 4), new Link<uint>(2, 5, 6))  // Duplicate
    //     };

    //     // Act
    //     var simplified = ChangesSimplifier.SimplifyChanges(changes).ToList();

    //     // Assert
    //     Assert.Equal(2, simplified.Count);

    //     Assert.Contains(simplified, c => c.Before.Index == 1 && c.After.Index == 1 && c.After.Source == 4 && c.After.Target == 5);
    //     Assert.Contains(simplified, c => c.Before.Index == 2 && c.After.Index == 2 && c.After.Source == 5 && c.After.Target == 6);
    // }

    // [Fact]
    // public void SimplifyChanges_NullChanges_ThrowsException()
    // {
    //     // Arrange
    //     List<(Link<uint> Before, Link<uint> After)> changes = null;

    //     // Act & Assert
    //     Assert.Throws<System.ArgumentNullException>(() => ChangesSimplifier.SimplifyChanges(changes).ToList());
    // }

    // [Fact]
    // public void SimplifyChanges_ChangesWithSameBeforeDifferentAfter_LastAfterIsRetained()
    // {
    //     // Arrange
    //     var changes = new List<(Link<uint> Before, Link<uint> After)>
    //     {
    //         (new Link<uint>(1, 2, 3), new Link<uint>(1, 4, 5)),
    //         (new Link<uint>(1, 2, 3), new Link<uint>(1, 6, 7)),
    //         (new Link<uint>(1, 2, 3), new Link<uint>(1, 8, 9))
    //     };

    //     // Act
    //     var simplified = ChangesSimplifier.SimplifyChanges(changes).ToList();

    //     // Assert
    //     Assert.Single(simplified);

    //     var link1 = simplified.First();
    //     Assert.Equal(new Link<uint>(1, 2, 3), link1.Before);
    //     Assert.Equal(new Link<uint>(1, 8, 9), link1.After); // Last change is retained
    // }
  }
}