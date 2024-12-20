using Platform.Delegates;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Protocols.Lino;
using LinoLink = Platform.Protocols.Lino.Link<string>;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

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
      var nullConstant = links.Constants.Null;
      var anyConstant = links.Constants.Any;
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
      if ((restrictionLink.Values?.Count == 0) && (substitutionLink.Values?.Count == 0))
      {
        return;
      }

      // If no restriction and we only have substitutions:
      if (restrictionLink.Values?.Count == 0 && (substitutionLink.Values?.Count ?? 0) > 0)
      {
        foreach (var linkToCreate in substitutionLink.Values ?? new List<LinoLink>())
        {
          // Instead of ConvertToDoubletLink + CreateOrUpdateLink, we now ensure recursion
          EnsureLinkAndSubLinksExist(links, linkToCreate, options);
        }
        return;
      }

      // If we have only restriction (delete) and no substitution:
      if (substitutionLink.Values?.Count == 0 && (restrictionLink.Values?.Count ?? 0) > 0)
      {
        foreach (var linkToDelete in restrictionLink.Values ?? new List<LinoLink>())
        {
          var queryLink = ConvertToDoubletLink(links, linkToDelete, anyConstant);
          RemoveLinks(links, queryLink, options);
        }
        return;
      }

      // Otherwise, handle both restriction and substitution:
      var restrictionPatterns = restrictionLink.Values ?? new List<LinoLink>();
      var substitutionPatterns = substitutionLink.Values ?? new List<LinoLink>();
      var restrictionInternalPatterns = restrictionPatterns.Select(l => CreatePatternFromLino(l)).ToList();
      var substitutionInternalPatterns = substitutionPatterns.Select(l => CreatePatternFromLino(l)).ToList();

      if (!string.IsNullOrEmpty(restrictionLink.Id))
      {
        restrictionInternalPatterns.Insert(0, CreatePatternFromLino(restrictionLink));
      }
      if (!string.IsNullOrEmpty(substitutionLink.Id))
      {
        substitutionInternalPatterns.Insert(0, CreatePatternFromLino(substitutionLink));
      }

      var solutions = FindAllSolutions(links, restrictionInternalPatterns);
      if (solutions.Count == 0)
      {
        return;
      }

      bool allSolutionsNoOperation = solutions.All(solution =>
          DetermineIfSolutionIsNoOperation(solution, restrictionInternalPatterns, substitutionInternalPatterns, links));
      var allPlannedOperations = new List<(DoubletLink before, DoubletLink after)>();

      if (allSolutionsNoOperation)
      {
        foreach (var solution in solutions)
        {
          var matchedLinks = ExtractMatchedLinks(links, solution, restrictionInternalPatterns);
          foreach (var link in matchedLinks)
          {
            allPlannedOperations.Add((link, link));
          }
        }
      }
      else
      {
        foreach (var solution in solutions)
        {
          // Before applying substitutions, ensure that any newly referenced links are created.
          // We will handle creation of substitution links now by ensuring that if substitutions contain
          // patterns that reference non-existent links or require creation, they will be created.
          var substitutionLinks = substitutionInternalPatterns
              .Select(pattern => ApplySolutionToPattern(links, solution, pattern))
              .ToList();
          var restrictionLinks = restrictionInternalPatterns
              .Select(pattern => ApplySolutionToPattern(links, solution, pattern))
              .ToList();

          var operations = DetermineOperationsFromPatterns(restrictionLinks, substitutionLinks);
          allPlannedOperations.AddRange(operations);
        }
      }

      if (allSolutionsNoOperation)
      {
        foreach (var (before, after) in allPlannedOperations)
        {
          options.ChangesHandler?.Invoke(before, after);
        }
      }
      else
      {
        var intendedFinalStates = new Dictionary<uint, DoubletLink>();
        foreach (var (before, after) in allPlannedOperations)
        {
          if (after.Index != 0)
          {
            intendedFinalStates[after.Index] = after;
          }
          else if (before.Index != 0 && after.Index == 0)
          {
            intendedFinalStates[before.Index] = default(DoubletLink);
          }
        }
        var unexpectedDeletions = new List<DoubletLink>();
        var originalHandler = options.ChangesHandler;
        options.ChangesHandler = (before, after) =>
        {
          var beforeLink = new DoubletLink(before);
          var afterLink = new DoubletLink(after);
          if (beforeLink.Index != 0 && afterLink.Index == 0)
          {
            bool isExpected = allPlannedOperations.Any(op => op.before.Index == beforeLink.Index && op.after.Index == 0);
            if (!isExpected)
            {
              unexpectedDeletions.Add(new DoubletLink(beforeLink));
            }
          }
          return originalHandler?.Invoke(before, after) ?? links.Constants.Continue;
        };
        ApplyAllPlannedOperations(links, allPlannedOperations, options);
        RestoreUnexpectedLinkDeletions(links, unexpectedDeletions, intendedFinalStates, options);
      }
    }

    private static uint EnsureLinkAndSubLinksExist(ILinks<uint> links, LinoLink lino, Options options)
    {
      var constants = links.Constants;
      var nullConstant = constants.Null;

      // Parse index
      uint index = constants.Any;
      if (!string.IsNullOrEmpty(lino.Id))
      {
        TryParseLinkId(lino.Id, constants, ref index);
      }

      // Handle sublinks
      uint source = constants.Any;
      uint target = constants.Any;
      if (lino.Values != null)
      {
        if (lino.Values.Count == 1)
        {
          // Single value means self-link pattern: (index: value->value)
          var singleVal = lino.Values[0];
          uint singleValId = EnsureLinkAndSubLinksExist(links, singleVal, options);
          // If index was ANY, use singleValId for both index and reference:
          // But we must differentiate between a top-level link and a scalar. If the top-level says (1 1),
          // it means index=1, and singleVal=1. So (1:1->1).
          // If index is ANY, that means we had no assigned index, so we just create a link with source=singleValId and target=singleValId
          // and let CreateOrUpdateLink assign or find an ID.

          if (index == constants.Any && singleValId == constants.Any)
          {
            // Both are ANY - we must create a new link with some stable ID. Without a numeric ID, we must create a new link.
            // A self-reference link: (?: singleValId->singleValId). If singleValId is ANY still, it means no stable link.
            // In that case, create a minimal link. Since singleValId is ANY, create a new singleVal link.
            singleValId = CreateOrUpdateLinkAndGetId(links, new DoubletLink(0, constants.Null, constants.Null), options);
            // Now we have a stable singleValId, make a self-link out of it:
            return CreateOrUpdateLinkAndGetId(links, new DoubletLink(0, singleValId, singleValId), options);
          }

          if (index == constants.Any)
          {
            // index not given, but singleValId is stable
            source = singleValId;
            target = singleValId;
            return CreateOrUpdateLinkAndGetId(links, new DoubletLink(0, source, target), options);
          }
          else
          {
            // index given
            source = singleValId;
            target = singleValId;
            return CreateOrUpdateLinkAndGetId(links, new DoubletLink(index, source, target), options);
          }
        }
        else if (lino.Values.Count == 2)
        {
          // Two-value link: (index: source->target)
          var sourceLink = lino.Values[0];
          var targetLink = lino.Values[1];
          source = EnsureLinkAndSubLinksExist(links, sourceLink, options);
          target = EnsureLinkAndSubLinksExist(links, targetLink, options);

          return CreateOrUpdateLinkAndGetId(links, new DoubletLink(index, source, target), options);
        }
        else if (lino.Values.Count == 0)
        {
          // No values means this is just an index reference, or ANY.
          // If index is a number and not ANY, create a self-link if it doesn't exist:
          if (index == constants.Any)
          {
            // Just ANY - create a minimal link (like (0:0->0)) and return its ID
            return CreateOrUpdateLinkAndGetId(links, new DoubletLink(0, nullConstant, nullConstant), options);
          }
          else
          {
            // Index given but no children:
            // Interpret it as a self-link (index:index->index).
            return CreateOrUpdateLinkAndGetId(links, new DoubletLink(index, index, index), options);
          }
        }
      }

      // If we reached here, means lino had no Values (null)
      // If index is ANY, create a minimal link
      // If index is given, create a self link with (index:index->index)
      if (lino.Values == null)
      {
        if (index == constants.Any)
        {
          return CreateOrUpdateLinkAndGetId(links, new DoubletLink(0, nullConstant, nullConstant), options);
        }
        else
        {
          return CreateOrUpdateLinkAndGetId(links, new DoubletLink(index, index, index), options);
        }
      }

      // Should never reach here, but just in case:
      return constants.Any;
    }

    private static void RestoreUnexpectedLinkDeletions(
        ILinks<uint> links,
        List<DoubletLink> unexpectedDeletions,
        Dictionary<uint, DoubletLink> finalIntendedStates,
        Options options)
    {
      var deletionsToProcess = new List<DoubletLink>(unexpectedDeletions);
      foreach (var deletedLink in deletionsToProcess)
      {
        if (finalIntendedStates.TryGetValue(deletedLink.Index, out var intendedLink))
        {
          if (intendedLink.Index == 0) continue;
          if (!links.Exists(intendedLink.Index))
          {
            CreateOrUpdateLink(links, intendedLink, options);
          }
        }
      }
    }

    private static List<(DoubletLink before, DoubletLink after)> DetermineOperationsFromPatterns(
        List<DoubletLink> restrictions,
        List<DoubletLink> substitutions)
    {
      var restrictionByIndex = restrictions.Where(r => r.Index != 0).ToDictionary(d => d.Index, d => d);
      var substitutionByIndex = substitutions.Where(s => s.Index != 0).ToDictionary(d => d.Index, d => d);
      var allIndices = restrictionByIndex.Keys.Union(substitutionByIndex.Keys).ToList();
      var operations = new List<(DoubletLink before, DoubletLink after)>();
      foreach (var index in allIndices)
      {
        bool hasRestriction = restrictionByIndex.TryGetValue(index, out var restrictionLink);
        bool hasSubstitution = substitutionByIndex.TryGetValue(index, out var substitutionLink);

        if (hasRestriction && hasSubstitution)
        {
          if (restrictionLink.Source != substitutionLink.Source || restrictionLink.Target != substitutionLink.Target)
          {
            operations.Add((restrictionLink, substitutionLink));
          }
          else
          {
            operations.Add((restrictionLink, restrictionLink));
          }
        }
        else if (hasRestriction && !hasSubstitution)
        {
          operations.Add((restrictionLink, default(DoubletLink)));
        }
        else if (!hasRestriction && hasSubstitution)
        {
          operations.Add((default(DoubletLink), substitutionLink));
        }
      }
      return operations;
    }

    private static void ApplyAllPlannedOperations(
        ILinks<uint> links,
        List<(DoubletLink before, DoubletLink after)> operations,
        Options options)
    {
      foreach (var (before, after) in operations)
      {
        if (before.Index != 0 && after.Index == 0)
        {
          RemoveLinks(links, before, options);
        }
        else if (before.Index == 0 && after.Index != 0)
        {
          CreateOrUpdateLink(links, after, options);
        }
        else if (before.Index != 0 && after.Index != 0)
        {
          if (before.Source != after.Source || before.Target != after.Target)
          {
            if (before.Index == after.Index)
            {
              if (!links.Exists(after.Index))
              {
                LinksExtensions.EnsureCreated(links, after.Index);
              }
              links.Update(before, after, (b, a) =>
                  options.ChangesHandler?.Invoke(b, a) ?? links.Constants.Continue);
            }
            else
            {
              RemoveLinks(links, before, options);
              CreateOrUpdateLink(links, after, options);
            }
          }
          else
          {
            options.ChangesHandler?.Invoke(before, before);
          }
        }
      }
    }

    private static List<Dictionary<string, uint>> FindAllSolutions(ILinks<uint> links, List<Pattern> patterns)
    {
      var partialSolutions = new List<Dictionary<string, uint>> { new Dictionary<string, uint>() };
      foreach (var pattern in patterns)
      {
        var newSolutions = new List<Dictionary<string, uint>>();
        foreach (var solution in partialSolutions)
        {
          foreach (var match in MatchPattern(links, pattern, solution))
          {
            if (AreSolutionsCompatible(solution, match))
            {
              var combinedSolution = new Dictionary<string, uint>(solution);
              foreach (var assignment in match)
              {
                combinedSolution[assignment.Key] = assignment.Value;
              }
              newSolutions.Add(combinedSolution);
            }
          }
        }
        partialSolutions = newSolutions;
        if (partialSolutions.Count == 0)
        {
          break;
        }
      }
      return partialSolutions;
    }

    private static bool AreSolutionsCompatible(Dictionary<string, uint> existingSolution, Dictionary<string, uint> newAssignments)
    {
      foreach (var assignment in newAssignments)
      {
        if (existingSolution.TryGetValue(assignment.Key, out var existingValue))
        {
          if (existingValue != assignment.Value)
          {
            return false;
          }
        }
      }
      return true;
    }

    private static IEnumerable<Dictionary<string, uint>> MatchPattern(
        ILinks<uint> links,
        Pattern pattern,
        Dictionary<string, uint> currentSolution)
    {
      uint indexValue = ResolveId(links, pattern.Index, currentSolution);
      uint sourceValue = ResolveId(links, pattern.Source, currentSolution);
      uint targetValue = ResolveId(links, pattern.Target, currentSolution);
      var candidates = links.All(new DoubletLink(indexValue, sourceValue, targetValue));
      foreach (var link in candidates)
      {
        var candidateLink = new DoubletLink(link);
        var assignments = new Dictionary<string, uint>();
        if (IsVariable(pattern.Index) || pattern.Index == "*")
        {
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

    private static bool IsVariable(string identifier)
    {
      return !string.IsNullOrEmpty(identifier) && identifier.StartsWith("$");
    }

    private static uint ResolveId(ILinks<uint> links, string identifier, Dictionary<string, uint> currentSolution)
    {
      var resolved = links.Constants.Any;
      if (string.IsNullOrEmpty(identifier)) return resolved;
      if (currentSolution.TryGetValue(identifier, out var value))
      {
        return value;
      }
      if (IsVariable(identifier))
      {
        return resolved;
      }
      if (TryParseLinkId(identifier, links.Constants, ref resolved))
      {
        return resolved;
      }
      return resolved;
    }

    private static bool DetermineIfSolutionIsNoOperation(
        Dictionary<string, uint> solution,
        List<Pattern> restrictions,
        List<Pattern> substitutions,
        ILinks<uint> links)
    {
      var substitutedRestrictions = restrictions.Select(r => ApplySolutionToPattern(links, solution, r)).ToList();
      var substitutedSubstitutions = substitutions.Select(s => ApplySolutionToPattern(links, solution, s)).ToList();
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

    private static List<DoubletLink> ExtractMatchedLinks(
        ILinks<uint> links,
        Dictionary<string, uint> solution,
        List<Pattern> patterns)
    {
      var matchedLinks = new List<DoubletLink>();
      foreach (var pattern in patterns)
      {
        var appliedPattern = ApplySolutionToPattern(links, solution, pattern);
        var matches = links.All(appliedPattern);
        foreach (var match in matches)
        {
          matchedLinks.Add(new DoubletLink(match));
        }
      }
      return matchedLinks.Distinct().ToList();
    }

    private static DoubletLink ApplySolutionToPattern(
        ILinks<uint> links,
        Dictionary<string, uint> solution,
        Pattern pattern)
    {
      uint index = ResolveId(links, pattern.Index, solution);
      uint source = ResolveId(links, pattern.Source, solution);
      uint target = ResolveId(links, pattern.Target, solution);
      return new DoubletLink(index, source, target);
    }

    private static uint CreateOrUpdateLinkAndGetId(ILinks<uint> links, DoubletLink link, Options options)
    {
      var id = CreateOrUpdateLink(links, link, options);
      return id;
    }

    private static uint CreateOrUpdateLink(ILinks<uint> links, DoubletLink link, Options options)
    {
      var nullConstant = links.Constants.Null;
      var anyConstant = links.Constants.Any;

      uint finalId;
      if (link.Index != nullConstant && link.Index != 0 && link.Index != anyConstant)
      {
        finalId = link.Index;
        if (!links.Exists(finalId))
        {
          LinksExtensions.EnsureCreated(links, finalId);
          // Just created a placeholder link, now update to correct source & target
          links.Update(new DoubletLink(finalId, nullConstant, nullConstant), link, (before, after) =>
              options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue);
        }
        else
        {
          var existingLink = new DoubletLink(links.GetLink(finalId));
          if (existingLink.Source != link.Source || existingLink.Target != link.Target)
          {
            links.Update(existingLink, link, (before, after) =>
                options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue);
          }
          else
          {
            // No changes, just invoke handler
            options.ChangesHandler?.Invoke(existingLink, existingLink);
          }
        }
      }
      else
      {
        // No fixed index, search or create
        var existingIndex = links.SearchOrDefault(link.Source, link.Target);
        if (existingIndex == default)
        {
          finalId = links.CreateAndUpdate(link.Source, link.Target, (before, after) =>
              options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue);
        }
        else
        {
          finalId = existingIndex;
          var existingLink = new DoubletLink(existingIndex, link.Source, link.Target);
          options.ChangesHandler?.Invoke(existingLink, existingLink);
        }
      }
      return finalId;
    }

    private static void RemoveLinks(ILinks<uint> links, DoubletLink restriction, Options options)
    {
      var linksToRemove = links.All(restriction);
      foreach (var link in linksToRemove)
      {
        if (links.Exists(link![0]))
        {
          links.Delete(link, (before, after) =>
              options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue);
        }
      }
    }

    private static DoubletLink ConvertToDoubletLink(ILinks<uint> links, LinoLink linoLink, uint defaultValue)
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

    private static bool TryParseLinkId(string? id, LinksConstants<uint> constants, ref uint parsedValue)
    {
      if (string.IsNullOrEmpty(id))
      {
        return false;
      }
      if (id == "*")
      {
        parsedValue = constants.Any;
        return true;
      }
      else if (uint.TryParse(id, out uint linkId))
      {
        parsedValue = linkId;
        return true;
      }
      return false;
    }

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

    private static Pattern CreatePatternFromLino(LinoLink lino)
    {
      var index = lino.Id ?? "";
      string source = "";
      string target = "";
      if (lino.Values?.Count == 2)
      {
        source = lino.Values[0].Id ?? "";
        target = lino.Values[1].Id ?? "";
      }
      else if (lino.Values?.Count == 1)
      {
        // Fallback: If we have only one value, interpret it as a self-link (id -> id).
        // e.g. (1 1) means Id="1", and a single value with Id="1",
        // so interpret source=1, target=1.
        var singleVal = lino.Values[0].Id;
        if (!string.IsNullOrEmpty(singleVal))
        {
          source = singleVal;
          target = singleVal;
        }
      }
      return new Pattern(index, source, target);
    }
  }
}