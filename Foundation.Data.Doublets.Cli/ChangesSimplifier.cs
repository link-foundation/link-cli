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
    /// Simplifies a list of changes by identifying chains of transformations from initial links to final links.
    /// If multiple final states are reachable from the same initial state, returns multiple simplified changes.
    /// </summary>
    /// <param name="changes">List of tuples representing changes (before, after).</param>
    /// <returns>Simplified list of changes from initial states to final states.</returns>
    public static IEnumerable<(DoubletLink Before, DoubletLink After)> SimplifyChanges(
        IEnumerable<(DoubletLink Before, DoubletLink After)> changes)
    {
      if (changes == null) throw new ArgumentNullException(nameof(changes));

      // Collect all before and after links
      var beforeLinks = new HashSet<DoubletLink>(changes.Select(c => c.Before), LinkEqualityComparer.Instance);
      var afterLinks = new HashSet<DoubletLink>(changes.Select(c => c.After), LinkEqualityComparer.Instance);

      // Identify initial states: appear as before but never as after
      var initialStates = beforeLinks.Where(b => !afterLinks.Contains(b)).ToList();

      // Identify final states: appear as after but never as before
      // (We use this info to know where chains can end.)
      var finalStates = afterLinks.Where(a => !beforeLinks.Contains(a)).ToHashSet(LinkEqualityComparer.Instance);

      // Build graph: from a Before link to one or more After links
      var adjacency = new Dictionary<DoubletLink, List<DoubletLink>>(LinkEqualityComparer.Instance);
      foreach (var (before, after) in changes)
      {
        if (!adjacency.TryGetValue(before, out var list))
        {
          list = new List<DoubletLink>();
          adjacency[before] = list;
        }
        list.Add(after);
      }

      var results = new List<(DoubletLink Before, DoubletLink After)>();

      // For each initial state, find all reachable final states
      foreach (var initial in initialStates)
      {
        // We'll do a DFS or BFS to find all final states from 'initial'
        var stack = new Stack<DoubletLink>();
        stack.Push(initial);
        var visited = new HashSet<DoubletLink>(LinkEqualityComparer.Instance);

        while (stack.Count > 0)
        {
          var current = stack.Pop();
          if (!visited.Add(current))
          {
            // Already visited, skip
            continue;
          }

          // If current is a final state, record the simplified transition initial -> current 
          // (if they differ; if not differ, maybe no change is needed, but let's produce anyway if asked)
          if (finalStates.Contains(current))
          {
            // Add result only if initial != final to reflect a change.
            // The tests expect a change when there's a chain. If initial == current and final,
            // it means no transformation happened. Check if the tests require filtering this out.
            if (!AreLinksEqual(initial, current))
            {
              results.Add((initial, current));
            }
          }

          // Traverse adjacency
          if (adjacency.TryGetValue(current, out var nextLinks))
          {
            foreach (var next in nextLinks)
            {
              stack.Push(next);
            }
          }
        }
      }

      return results;
    }

    private static bool AreLinksEqual(DoubletLink a, DoubletLink b)
    {
      return a.Index == b.Index && a.Source == b.Source && a.Target == b.Target;
    }

    private class LinkEqualityComparer : IEqualityComparer<DoubletLink>
    {
      public static readonly LinkEqualityComparer Instance = new LinkEqualityComparer();
      public bool Equals(DoubletLink x, DoubletLink y) => x.Index == y.Index && x.Source == y.Source && x.Target == y.Target;
      public int GetHashCode(DoubletLink obj) => HashCode.Combine(obj.Index, obj.Source, obj.Target);
    }
  }
}