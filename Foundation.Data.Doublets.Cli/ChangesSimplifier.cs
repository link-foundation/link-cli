using System;
using System.Collections.Generic;
using System.Linq;
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

      // Gather all 'Before' links and all 'After' links
      var beforeLinks = new HashSet<Link<uint>>(changesList.Select(c => c.Before), LinkEqualityComparer.Instance);
      var afterLinks = new HashSet<Link<uint>>(changesList.Select(c => c.After), LinkEqualityComparer.Instance);

      // Identify initial states: appear as Before but never as After
      var initialStates = beforeLinks.Where(b => !afterLinks.Contains(b)).ToList();

      // Identify final states: appear as After but never as Before
      var finalStates = afterLinks.Where(a => !beforeLinks.Contains(a))
                                  .ToHashSet(LinkEqualityComparer.Instance);

      // Build adjacency (Before -> possible list of After links)
      var adjacency = new Dictionary<Link<uint>, List<Link<uint>>>(LinkEqualityComparer.Instance);
      foreach (var (before, after) in changesList)
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