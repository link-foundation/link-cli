using System;
using System.Collections.Generic;
using System.Linq;
using Platform.Data.Doublets;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

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
    /// <returns>Simplified list of changes from initial states to final states, or original transitions if no change is detected.</returns>
    public static IEnumerable<(DoubletLink Before, DoubletLink After)> SimplifyChanges(
        IEnumerable<(DoubletLink Before, DoubletLink After)> changes)
    {
      if (changes == null) throw new ArgumentNullException(nameof(changes));

      var changesList = changes.ToList();
      if (changesList.Count == 0)
      {
        // No changes at all, return empty
        return Enumerable.Empty<(DoubletLink, DoubletLink)>();
      }

      var beforeLinks = new HashSet<DoubletLink>(changesList.Select(c => c.Before), LinkEqualityComparer.Instance);
      var afterLinks = new HashSet<DoubletLink>(changesList.Select(c => c.After), LinkEqualityComparer.Instance);

      // Initial states: appear as Before but never as After
      var initialStates = beforeLinks.Where(b => !afterLinks.Contains(b)).ToList();

      // Final states: appear as After but never as Before
      var finalStates = afterLinks.Where(a => !beforeLinks.Contains(a)).ToHashSet(LinkEqualityComparer.Instance);

      var adjacency = new Dictionary<DoubletLink, List<DoubletLink>>(LinkEqualityComparer.Instance);
      foreach (var (before, after) in changesList)
      {
        if (!adjacency.TryGetValue(before, out var list))
        {
          list = new List<DoubletLink>();
          adjacency[before] = list;
        }
        list.Add(after);
      }

      // If we have no identified initial states, it might be a no-op scenario.
      // For no-op scenario, there's no "initial" or "final" distinct link,
      // which means every link that appears as Before also appears as After identically.
      // In this case, we must return the original transitions as requested.
      if (initialStates.Count == 0)
      {
        // Just return all given changes. They represent no-ops/read operations.
        return changesList;
      }

      var results = new List<(DoubletLink Before, DoubletLink After)>();

      foreach (var initial in initialStates.Distinct(LinkEqualityComparer.Instance))
      {
        var stack = new Stack<DoubletLink>();
        stack.Push(initial);
        var visited = new HashSet<DoubletLink>(LinkEqualityComparer.Instance);

        while (stack.Count > 0)
        {
          var current = stack.Pop();
          if (!visited.Add(current))
          {
            continue;
          }

          bool hasNext = adjacency.TryGetValue(current, out var nextLinks);
          bool isFinalOrDeadEnd = finalStates.Contains(current) || !hasNext || nextLinks!.Count == 0;

          if (isFinalOrDeadEnd)
          {
            // Record the transition from initial to current, even if identical (no-op).
            results.Add((initial, current));
          }

          if (hasNext)
          {
            foreach (var next in nextLinks!)
            {
              stack.Push(next);
            }
          }
        }
      }

      return results;
    }

    private class LinkEqualityComparer : IEqualityComparer<DoubletLink>
    {
      public static readonly LinkEqualityComparer Instance = new LinkEqualityComparer();
      public bool Equals(DoubletLink x, DoubletLink y) => x.Index == y.Index && x.Source == y.Source && x.Target == y.Target;
      public int GetHashCode(DoubletLink obj) => HashCode.Combine(obj.Index, obj.Source, obj.Target);
    }
  }
}