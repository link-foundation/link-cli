using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli
{
  public static class ChangesSimplifier
  {
    /// <summary>
    /// Simplifies a list of changes by identifying chains of transformations.
    /// If multiple final states are reachable from the same initial state, returns multiple simplified changes.
    /// If a scenario arises where no initial or final states can be identified (no-ops), returns the original transitions as-is.
    /// </summary>
    /// <param name="changes">List of tuples representing changes (before, after).</param>
    /// <returns>
    /// Simplified list of changes from initial states to final states,
    /// or original transitions if no change is detected.
    /// </returns>
    public static IEnumerable<(Link<uint> Before, Link<uint> After)> SimplifyChanges(
        IEnumerable<(Link<uint> Before, Link<uint> After)> changes
    )
    {
      if (changes == null) throw new ArgumentNullException(nameof(changes));

      var changesList = changes.ToList();
      if (changesList.Count == 0)
      {
        // No changes at all, return empty
        return Enumerable.Empty<(Link<uint>, Link<uint>)>();
      }

      // **FIX for Issue #26**: Remove duplicate before states by keeping the last occurrence
      // This handles cases where the same link is reported with multiple different transformations
      changesList = RemoveDuplicateBeforeStates(changesList);

      // First, handle unchanged states directly
      var unchangedStates = new List<(Link<uint> Before, Link<uint> After)>();
      var changedStates = new List<(Link<uint> Before, Link<uint> After)>();

      foreach (var change in changesList)
      {
        if (LinkEqualityComparer.Instance.Equals(change.Before, change.After))
        {
          unchangedStates.Add(change);
        }
        else
        {
          changedStates.Add(change);
        }
      }

      // Gather all 'Before' links and all 'After' links from changed states
      var beforeLinks = new HashSet<Link<uint>>(changedStates.Select(c => c.Before), LinkEqualityComparer.Instance);
      var afterLinks = new HashSet<Link<uint>>(changedStates.Select(c => c.After), LinkEqualityComparer.Instance);

      // Identify initial states: appear as Before but never as After
      var initialStates = beforeLinks.Where(b => !afterLinks.Contains(b)).ToList();

      // Identify final states: appear as After but never as Before
      var finalStates = afterLinks.Where(a => !beforeLinks.Contains(a))
                                  .ToHashSet(LinkEqualityComparer.Instance);

      // Build adjacency (Before -> possible list of After links)
      var adjacency = new Dictionary<Link<uint>, List<Link<uint>>>(LinkEqualityComparer.Instance);
      foreach (var (before, after) in changedStates)
      {
        if (!adjacency.TryGetValue(before, out var list))
        {
          list = new List<Link<uint>>();
          adjacency[before] = list;
        }
        list.Add(after);
      }

      // If we have no identified initial states, treat it as a no-op scenario:
      // just return original transitions.
      if (initialStates.Count == 0)
      {
        return changesList;
      }

      var results = new List<(Link<uint> Before, Link<uint> After)>();

      // Add unchanged states first
      results.AddRange(unchangedStates);

      // Traverse each initial state with DFS
      foreach (var initial in initialStates.Distinct(LinkEqualityComparer.Instance))
      {
        var stack = new Stack<Link<uint>>();
        stack.Push(initial);

        var visited = new HashSet<Link<uint>>(LinkEqualityComparer.Instance);

        while (stack.Count > 0)
        {
          var current = stack.Pop();
          // Skip if already visited
          if (!visited.Add(current))
          {
            continue;
          }

          bool hasNext = adjacency.TryGetValue(current, out var nextLinks);
          bool isFinalOrDeadEnd = finalStates.Contains(current) || !hasNext || nextLinks!.Count == 0;

          // If final or no further transitions, record (initial -> current)
          if (isFinalOrDeadEnd)
          {
            results.Add((initial, current));
          }

          // Otherwise push neighbors
          if (hasNext)
          {
            foreach (var next in nextLinks!)
            {
              stack.Push(next);
            }
          }
        }
      }

      // ***** IMPORTANT: Sort the final results so that
      // items appear in ascending order by their After link.
      // This ensures tests that expect a specific order pass reliably.
      return results
          .OrderBy(r => r.After.Index)
          .ThenBy(r => r.After.Source)
          .ThenBy(r => r.After.Target);
    }

    /// <summary>
    /// Removes problematic duplicate before states that lead to simplification issues.
    /// This fixes Issue #26 where multiple transformations from the same before state
    /// to conflicting after states (including null states) would cause the simplifier to fail.
    /// 
    /// The key insight: If we have multiple transitions from the same before state,
    /// and one of them is to a "null" state (0: 0 0), we should prefer the non-null transition
    /// as it represents the actual final transformation.
    /// </summary>
    /// <param name="changes">The list of changes that may contain problematic duplicate before states</param>
    /// <returns>A list with problematic duplicates resolved</returns>
    private static List<(Link<uint> Before, Link<uint> After)> RemoveDuplicateBeforeStates(
        List<(Link<uint> Before, Link<uint> After)> changes)
    {
      // Group changes by their before state
      var groupedChanges = changes.GroupBy(c => c.Before, LinkEqualityComparer.Instance);
      
      var result = new List<(Link<uint> Before, Link<uint> After)>();
      
      foreach (var group in groupedChanges)
      {
        var changesForThisBefore = group.ToList();
        
        if (changesForThisBefore.Count == 1)
        {
          // No duplicates, keep as is
          result.AddRange(changesForThisBefore);
        }
        else
        {
          // Multiple changes from the same before state
          // Check if any of them is to a null state (0: 0 0)
          var nullTransition = changesForThisBefore.FirstOrDefault(c => 
              c.After.Index == 0 && c.After.Source == 0 && c.After.Target == 0);
          var nonNullTransitions = changesForThisBefore.Where(c => 
              !(c.After.Index == 0 && c.After.Source == 0 && c.After.Target == 0)).ToList();
          
          if (nullTransition != default && nonNullTransitions.Count > 0)
          {
            // Issue #26 scenario: We have both null and non-null transitions
            // Prefer the non-null transitions as they represent the actual final states
            result.AddRange(nonNullTransitions);
          }
          else
          {
            // No null transitions involved, this is a legitimate multiple-branch scenario
            // Keep all transitions
            result.AddRange(changesForThisBefore);
          }
        }
      }
      
      return result;
    }

    /// <summary>
    /// An equality comparer for Link<uint> that checks Index/Source/Target.
    /// </summary>
    private class LinkEqualityComparer : IEqualityComparer<Link<uint>>
    {
      public static readonly LinkEqualityComparer Instance = new LinkEqualityComparer();

      public bool Equals(Link<uint> x, Link<uint> y)
          => x.Index == y.Index && x.Source == y.Source && x.Target == y.Target;

      public int GetHashCode(Link<uint> obj)
          => HashCode.Combine(obj.Index, obj.Source, obj.Target);
    }
  }




}