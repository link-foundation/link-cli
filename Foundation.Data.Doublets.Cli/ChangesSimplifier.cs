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
    /// Simplifies the list of changes by removing intermediate states.
    /// For each unique initial link, only the transition from the first 'Before' to the last 'After' is retained.
    /// </summary>
    /// <param name="changes">List of tuples representing changes (before, after).</param>
    /// <returns>Simplified list of changes.</returns>
    public static IEnumerable<(DoubletLink Before, DoubletLink After)> SimplifyChanges(
        IEnumerable<(DoubletLink Before, DoubletLink After)> changes)
    {
      if (changes == null)
        throw new ArgumentNullException(nameof(changes));

      // Dictionaries to track initial 'Before' and final 'After' states
      var initialBeforeStates = new Dictionary<uint, DoubletLink>();
      var finalAfterStates = new Dictionary<uint, DoubletLink>();

      foreach (var (before, after) in changes)
      {
        if (!initialBeforeStates.ContainsKey(before.Index))
        {
          // Store the first 'Before' state for each unique index
          initialBeforeStates[before.Index] = before;
        }

        // Always update to the latest 'After' state
        finalAfterStates[before.Index] = after;
      }

      // Combine initial 'Before' and final 'After' states
      return initialBeforeStates.Keys.Select(index => (Before: initialBeforeStates[index], After: finalAfterStates[index]));
    }
  }
}