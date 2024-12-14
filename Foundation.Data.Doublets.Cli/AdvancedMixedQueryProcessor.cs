using Platform.Data.Doublets;
using Platform.Protocols.Lino;
using LinoLink = Platform.Protocols.Lino.Link<string>;
using DoubletLink = Platform.Data.Doublets.Link<uint>;
using System.Numerics;
using Platform.Data;
using Platform.Delegates;
using System.Collections.Generic;
using System.Linq;
using Platform.Converters;

namespace Foundation.Data.Doublets.Cli
{
  public static class AdvancedMixedQueryProcessor
  {
    public class Options
    {
      public string? Query { get; set; }
      public WriteHandler<uint>? ChangesHandler { get; set; }

      public static implicit operator Options(string query) => new Options { Query = query };
    }

    public static void ProcessQuery(ILinks<uint> links, Options options)
    {
      var query = options.Query;
      var @null = links.Constants.Null;
      var any = links.Constants.Any;
      if (string.IsNullOrEmpty(query))
      {
        return;
      }

      var parser = new Parser();
      var parsedLinks = parser.Parse(query);

      if (parsedLinks.Count == 0)
      {
        return;
      }

      var outerLink = parsedLinks[0];
      var outerLinkValues = outerLink.Values;
      if (outerLinkValues?.Count < 2)
      {
        return;
      }

      var restrictionLink = outerLinkValues![0];
      var substitutionLink = outerLinkValues![1];

      // If both sides are empty, no operation
      if ((restrictionLink.Values?.Count == 0) && (substitutionLink.Values?.Count == 0))
      {
        return;
      }

      // Handle create/delete/update scenarios
      if (restrictionLink.Values?.Count == 0 && substitutionLink.Values?.Count > 0)
      {
        // Create new links
        foreach (var linkToCreate in substitutionLink.Values ?? new List<LinoLink>())
        {
          var doubletLink = ToDoubletLink(links, linkToCreate, @null);
          Set(links, doubletLink, options);
        }
        return;
      }

      if (substitutionLink.Values?.Count == 0 && restrictionLink.Values?.Count > 0)
      {
        // Delete links
        foreach (var linkToDelete in restrictionLink.Values ?? new List<LinoLink>())
        {
          var queryLink = ToDoubletLink(links, linkToDelete, any);
          Unset(links, queryLink, options);
        }
        return;
      }

      // Now handle the scenario where both restriction and substitution have values.
      // This implies a more complex pattern with possible variables and enumerations.
      var restrictionPatterns = restrictionLink.Values ?? new List<LinoLink>();
      var substitutionPatterns = substitutionLink.Values ?? new List<LinoLink>();

      // Convert patterns into an internal form for matching
      // We'll unify variables across all restriction patterns.
      var restrictionPatternsInternal = restrictionPatterns.Select(l => PatternFromLino(links, l)).ToList();
      var substitutionPatternsInternal = substitutionPatterns.Select(l => PatternFromLino(links, l)).ToList();

      // If restriction and substitution have an Id themselves, consider them as well
      if (!string.IsNullOrEmpty(restrictionLink.Id))
      {
        restrictionPatternsInternal.Insert(0, PatternFromLino(links, restrictionLink));
      }
      if (!string.IsNullOrEmpty(substitutionLink.Id))
      {
        substitutionPatternsInternal.Insert(0, PatternFromLino(links, substitutionLink));
      }

      // Find all solutions that satisfy the restriction patterns
      var solutions = FindAllSolutions(links, restrictionPatternsInternal);

      if (solutions.Count == 0)
      {
        // No matches found, do nothing
        return;
      }

      // For each solution, apply the substitution
      // If substitution = restriction (no difference), treat as read
      bool isNoOp = CheckIfNoOp(solutions[0], restrictionPatternsInternal, substitutionPatternsInternal);

      if (isNoOp)
      {
        // Just read and call ChangesHandler
        // For each solution and each matched link, invoke ChangesHandler
        foreach (var solution in solutions)
        {
          // Each pattern or the main matched set of links can be processed:
          // Here we consider all matched links from the solution's assignments.
          // The solution should have assigned values to variables or found specific links.
          var matchedLinks = ExtractMatchedLinksFromSolution(links, solution, restrictionPatternsInternal);
          foreach (var link in matchedLinks)
          {
            options.ChangesHandler?.Invoke(link, link);
          }
        }
        return;
      }

      // If not no-op, we apply substitution for each solution.
      // The substitution patterns may contain variables that are now bound by the solution.
      foreach (var solution in solutions)
      {
        // Build actual substitution doublets from the substitutionPatternsInternal using the solution
        var substitutionDoublets = substitutionPatternsInternal
            .Select(pattern => ApplySolutionToPattern(links, solution, pattern))
            .ToList();

        // Similarly, get the original matched links from the restriction patterns
        var restrictionDoublets = restrictionPatternsInternal
            .Select(pattern => ApplySolutionToPattern(links, solution, pattern))
            .ToList();

        // Now we have a set of old (restrictionDoublets) and new (substitutionDoublets).
        // We compare by index or by variable ID sets to decide updates, sets, and unsets.
        ApplyChanges(links, restrictionDoublets, substitutionDoublets, options);
      }
    }

    #region Helper Methods for the New Design

    private static List<Dictionary<string, uint>> FindAllSolutions(ILinks<uint> links, List<Pattern> patterns)
    {
      // This function attempts to find all solutions that satisfy all patterns simultaneously.
      // It will implement a backtracking approach:
      // 1. Start with an empty assignment dictionary.
      // 2. Match the first pattern, enumerating all possible matches.
      // 3. For each match, unify variables and proceed to the next pattern.
      // 4. Only keep assignments that satisfy all patterns.

      var partialSolutions = new List<Dictionary<string, uint>> { new Dictionary<string, uint>() };

      foreach (var pattern in patterns)
      {
        var newSolutions = new List<Dictionary<string, uint>>();
        foreach (var solution in partialSolutions)
        {
          foreach (var match in MatchPattern(links, pattern, solution))
          {
            // match is a dictionary of variable assignments
            // unify with current solution
            if (Unify(solution, match))
            {
              // If unification succeeds, we get a combined solution
              var combined = new Dictionary<string, uint>(solution);
              foreach (var kv in match)
              {
                combined[kv.Key] = kv.Value;
              }
              newSolutions.Add(combined);
            }
          }
        }
        partialSolutions = newSolutions;
        if (partialSolutions.Count == 0)
        {
          // No solutions at some step, abort
          break;
        }
      }

      return partialSolutions;
    }

    private static bool Unify(Dictionary<string, uint> currentSolution, Dictionary<string, uint> newAssignments)
    {
      foreach (var kv in newAssignments)
      {
        if (currentSolution.TryGetValue(kv.Key, out var existingVal))
        {
          // Variable already assigned, must match the new value
          if (existingVal != kv.Value)
          {
            return false; // conflict
          }
        }
        // else it's a new variable assignment, which is fine
      }
      return true;
    }

    private static IEnumerable<Dictionary<string, uint>> MatchPattern(ILinks<uint> links, Pattern pattern, Dictionary<string, uint> currentSolution)
    {
      // Match a single pattern:
      // If pattern uses * or variable for index/source/target, enumerate links
      // Otherwise, match a single known link or fail.

      var any = links.Constants.Any;

      // Determine the link query from pattern
      // Resolve variables already assigned in currentSolution if present
      uint indexVal = ResolveId(links, pattern.Index, currentSolution);
      uint sourceVal = ResolveId(links, pattern.Source, currentSolution);
      uint targetVal = ResolveId(links, pattern.Target, currentSolution);

      // If indexVal, sourceVal, targetVal are known constants (not ANY) we can do a direct query
      var candidates = links.All(new DoubletLink(indexVal, sourceVal, targetVal));

      foreach (var link in candidates)
      {
        // Now assign variables from this candidate
        var candidateLink = new DoubletLink(link);
        var assignments = new Dictionary<string, uint>();
        // If pattern.Index is a variable, assign
        if (IsVariable(pattern.Index) || pattern.Index == "*:")
        {
          // For *:, treat it like an enumerator. Actually it's just all links anyway.
          AssignVariable(pattern.Index, candidateLink.Index, assignments);
        }
        if (IsVariable(pattern.Source))
        {
          AssignVariable(pattern.Source, candidateLink.Source, assignments);
        }
        if (IsVariable(pattern.Target))
        {
          AssignVariable(pattern.Target, candidateLink.Target, assignments);
        }
        yield return assignments;
      }
    }

    private static void AssignVariable(string variableName, uint value, Dictionary<string, uint> assignments)
    {
      if (!string.IsNullOrEmpty(variableName) && variableName.StartsWith("$"))
      {
        assignments[variableName] = value;
      }
    }

    private static bool IsVariable(string id)
    {
      return !string.IsNullOrEmpty(id) && id.StartsWith("$");
    }

    private static uint ResolveId(ILinks<uint> links, string id, Dictionary<string, uint> currentSolution)
    {
      if (string.IsNullOrEmpty(id)) return links.Constants.Any;

      if (currentSolution.TryGetValue(id, out var val))
      {
        return val;
      }

      if (id == "*") return links.Constants.Any;
      if (id == "*:") return links.Constants.Any; // Means enumerate all links by index
      if (uint.TryParse(id, out var parsed))
      {
        return parsed;
      }

      // If it's a variable but not assigned yet, treat as ANY for now
      // Actual assignment will occur during match
      if (IsVariable(id))
      {
        return links.Constants.Any;
      }

      return links.Constants.Any;
    }

    private static bool CheckIfNoOp(Dictionary<string, uint> solution, List<Pattern> restrictions, List<Pattern> substitutions)
    {
      // Apply solution to both sets of patterns and see if they produce the same set of doublets.
      var substitutedRestrictions = restrictions.Select(r => ApplySolutionToPattern(null, solution, r)).ToList();
      var substitutedSubstitutions = substitutions.Select(s => ApplySolutionToPattern(null, solution, s)).ToList();

      // Compare sets
      // Sort by index to compare easily
      substitutedRestrictions.Sort((a, b) => a.Index.CompareTo(b.Index));
      substitutedSubstitutions.Sort((a, b) => a.Index.CompareTo(b.Index));

      if (substitutedRestrictions.Count != substitutedSubstitutions.Count) return false;
      for (int i = 0; i < substitutedRestrictions.Count; i++)
      {
        if (!substitutedRestrictions[i].Equals(substitutedSubstitutions[i]))
        {
          return false;
        }
      }
      return true;
    }

    private static List<DoubletLink> ExtractMatchedLinksFromSolution(ILinks<uint> links, Dictionary<string, uint> solution, List<Pattern> patterns)
    {
      // Apply solution and find all matched links
      var result = new List<DoubletLink>();
      foreach (var pattern in patterns)
      {
        var dbl = ApplySolutionToPattern(links, solution, pattern);
        // Query actual links that match dbl
        // dbl might have ANY for unknown, but solution should have resolved variables
        var candidates = links.All(dbl);
        foreach (var c in candidates)
        {
          result.Add(new DoubletLink(c));
        }
      }
      return result.Distinct().ToList();
    }

    private static DoubletLink ApplySolutionToPattern(ILinks<uint> links, Dictionary<string, uint> solution, Pattern pattern)
    {
      uint i = ApplyVarOrId(links, pattern.Index, solution);
      uint s = ApplyVarOrId(links, pattern.Source, solution);
      uint t = ApplyVarOrId(links, pattern.Target, solution);
      return new DoubletLink(i, s, t);
    }

    private static uint ApplyVarOrId(ILinks<uint> links, string id, Dictionary<string, uint> solution)
    {
      if (string.IsNullOrEmpty(id)) return links.Constants.Any;
      if (solution.TryGetValue(id, out var val)) return val;
      if (id == "*") return links.Constants.Any;
      if (id == "*:") return links.Constants.Any;
      if (uint.TryParse(id, out var parsed)) return parsed;
      return links.Constants.Any;
    }

    private static void ApplyChanges(ILinks<uint> links, List<DoubletLink> restrictions, List<DoubletLink> substitutions, Options options)
    {
      // We have matched multiple links. 
      // The logic: 
      // - If a link appears in restrictions but not in substitutions: delete it
      // - If a link appears in substitutions but not in restrictions: create it
      // - If a link appears in both but with different source/target: update it
      // - If same: no-op

      var rByIndex = restrictions.ToDictionary(d => d.Index);
      var sByIndex = substitutions.ToDictionary(d => d.Index);

      var allIndices = rByIndex.Keys.Union(sByIndex.Keys).ToList();

      foreach (var idx in allIndices)
      {
        var hasR = rByIndex.TryGetValue(idx, out var rlink);
        var hasS = sByIndex.TryGetValue(idx, out var slink);

        if (hasR && hasS)
        {
          // Update operation if different
          if (rlink.Source != slink.Source || rlink.Target != slink.Target)
          {
            links.Update(rlink, slink, (before, after) => options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue);
          }
          else
          {
            // No changes, just a read
            options.ChangesHandler?.Invoke(rlink, rlink);
          }
        }
        else if (hasR && !hasS)
        {
          // Delete
          Unset(links, rlink, options);
        }
        else if (!hasR && hasS)
        {
          // Create
          Set(links, slink, options);
        }
      }
    }

    private static Pattern PatternFromLino(ILinks<uint> links, LinoLink lino)
    {
      var index = lino.Id ?? "";
      string source = "";
      string target = "";
      if (lino.Values?.Count == 2)
      {
        source = lino.Values[0].Id ?? "";
        target = lino.Values[1].Id ?? "";
      }
      return new Pattern(index, source, target);
    }

    #endregion

    #region Existing Methods for Set/Unset/Read

    static void Set(ILinks<uint> links, DoubletLink substitutionLink, Options options)
    {
      var @null = links.Constants.Null;
      var any = links.Constants.Any;
      if (substitutionLink.Source == any)
      {
        throw new ArgumentException($"The source of the link {substitutionLink} cannot be any.");
      }
      if (substitutionLink.Target == any)
      {
        throw new ArgumentException($"The target of the link {substitutionLink} cannot be any.");
      }
      if (substitutionLink.Index != @null)
      {
        LinksExtensions.EnsureCreated(links, substitutionLink.Index);
        var restrictionDoublet = new DoubletLink(substitutionLink.Index, any, any);
        options.ChangesHandler?.Invoke(null, restrictionDoublet);
        links.Update(restrictionDoublet, substitutionLink, (before, after) =>
        {
          return options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;
        });
      }
      else
      {
        var linkIndex = links.SearchOrDefault(substitutionLink.Source, substitutionLink.Target);
        if (linkIndex == default)
        {
          linkIndex = links.CreateAndUpdate(substitutionLink.Source, substitutionLink.Target, (before, after) =>
          {
            return options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;
          });
        }
        else
        {
          var existingLink = new DoubletLink(linkIndex, substitutionLink.Source, substitutionLink.Target);
          options.ChangesHandler?.Invoke(existingLink, existingLink);
        }
      }
    }

    static void Unset(ILinks<uint> links, DoubletLink restrictionLink, Options options)
    {
      var linksToDelete = links.All(restrictionLink);
      foreach (var link in linksToDelete)
      {
        links.Delete(link, (before, after) =>
        {
          return options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;
        });
      }
    }

    static DoubletLink ToDoubletLink(ILinks<uint> links, LinoLink linoLink, uint defaultValue)
    {
      uint index = defaultValue;
      uint source = defaultValue;
      uint target = defaultValue;
      TryParseLinkId(linoLink.Id, links.Constants, ref index);
      if (linoLink.Values?.Count == 2)
      {
        var sourceLink = linoLink.Values[0];
        TryParseLinkId(sourceLink.Id, links.Constants, ref source);
        var targetLink = linoLink.Values[1];
        TryParseLinkId(targetLink.Id, links.Constants, ref target);
      }
      return new DoubletLink(index, source, target);
    }

    static void TryParseLinkId(string? id, LinksConstants<uint> constants, ref uint parsedValue)
    {
      if (string.IsNullOrEmpty(id))
      {
        return;
      }
      if (id == "*")
      {
        parsedValue = constants.Any;
      }
      else if (uint.TryParse(id, out uint linkId))
      {
        parsedValue = linkId;
      }
    }

    #endregion

    #region Supporting Classes

    public class Pattern
    {
      public string Index;
      public string Source;
      public string Target;
      public Pattern(string index, string source, string target)
      {
        Index = index ?? "";
        Source = source ?? "";
        Target = target ?? "";
      }
    }



    #endregion
  }
}
