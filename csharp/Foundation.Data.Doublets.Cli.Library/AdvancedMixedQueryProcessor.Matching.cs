// Part of the partial AdvancedMixedQueryProcessor class.
// Pattern matching: turning restriction patterns into concrete variable solutions.
using System;
using Platform.Delegates;
using Platform.Data;
using Platform.Data.Doublets;
using Link.Foundation.Links.Notation;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;
using DoubletLink = Platform.Data.Doublets.Link<uint>;
namespace Foundation.Data.Doublets.Cli
{
    public static partial class AdvancedMixedQueryProcessor
    {
        private static List<Dictionary<string, uint>> FindAllSolutions(INamedTypesLinks<uint> links, List<Pattern> patterns)
        {
            var partialSolutions = new List<Dictionary<string, uint>> { new Dictionary<string, uint>() };

            for (int i = 0; i < patterns.Count; i++)
            {
                var pattern = patterns[i];
                var newSolutions = new List<Dictionary<string, uint>>();
                foreach (var solution in partialSolutions)
                {
                    var matches = MatchPattern(links, pattern, solution).ToList();
                    foreach (var match in matches)
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
                if (partialSolutions.Count == 0) break;
            }

            return partialSolutions;
        }

        private static bool AreSolutionsCompatible(
            Dictionary<string, uint> existingSolution,
            Dictionary<string, uint> newAssignments)
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
            INamedTypesLinks<uint> links,
            Pattern pattern,
            Dictionary<string, uint> currentSolution)
        {
            var anyConstant = links.Constants.Any;
            if (pattern.IsLeaf)
            {
                uint leafIndex = ResolveId(links, pattern.Index, currentSolution);
                var candidates = links.All(new DoubletLink(leafIndex, anyConstant, anyConstant));
                foreach (var link in candidates)
                {
                    var candidateLink = new DoubletLink(link);
                    var assignments = new Dictionary<string, uint>();
                    AssignVariableIfNeeded(pattern.Index, candidateLink.Index, assignments);
                    yield return assignments;
                }
                yield break;
            }

            bool indexIsVariable = IsVariable(pattern.Index);
            bool indexIsAny = pattern.Index == "*";
            uint resolvedIndex = ResolveId(links, pattern.Index, currentSolution);

            // If idxResolved is a known link => skip enumerating everything
            if (!indexIsVariable && !indexIsAny && resolvedIndex != anyConstant && resolvedIndex != 0 && links.Exists(resolvedIndex))
            {
                var link = new DoubletLink(links.GetLink(resolvedIndex));
                var sourceMatches = RecursiveMatchSubPattern(links, pattern.Source, link.Source, currentSolution);
                foreach (var sourceSolution in sourceMatches)
                {
                    var targetMatches = RecursiveMatchSubPattern(links, pattern.Target, link.Target, sourceSolution);
                    foreach (var targetSolution in targetMatches)
                    {
                        var combined = new Dictionary<string, uint>(targetSolution);
                        AssignVariableIfNeeded(pattern.Index, resolvedIndex, combined);
                        yield return combined;
                    }
                }
            }
            else
            {
                // Otherwise we iterate over all links
                var allLinks = links.All(new DoubletLink(anyConstant, anyConstant, anyConstant));
                foreach (var raw in allLinks)
                {
                    var candidateLink = new DoubletLink(raw);
                    if (!CheckIdMatch(links, pattern.Index, candidateLink.Index, currentSolution))
                        continue;

                    var sourceMatches = RecursiveMatchSubPattern(links, pattern.Source, candidateLink.Source, currentSolution);
                    foreach (var sourceSolution in sourceMatches)
                    {
                        var targetMatches = RecursiveMatchSubPattern(links, pattern.Target, candidateLink.Target, sourceSolution);
                        foreach (var targetSolution in targetMatches)
                        {
                            var combined = new Dictionary<string, uint>(targetSolution);
                            AssignVariableIfNeeded(pattern.Index, candidateLink.Index, combined);
                            yield return combined;
                        }
                    }
                }
            }
        }

        private static IEnumerable<Dictionary<string, uint>> RecursiveMatchSubPattern(
            INamedTypesLinks<uint> links,
            Pattern? pattern,
            uint linkId,
            Dictionary<string, uint> currentSolution)
        {
            if (pattern == null)
            {
                yield return currentSolution;
                yield break;
            }

            if (pattern.IsLeaf)
            {
                if (CheckIdMatch(links, pattern.Index, linkId, currentSolution))
                {
                    var newSolution = new Dictionary<string, uint>(currentSolution);
                    AssignVariableIfNeeded(pattern.Index, linkId, newSolution);
                    yield return newSolution;
                }
                yield break;
            }

            if (!links.Exists(linkId)) yield break;

            var link = new DoubletLink(links.GetLink(linkId));
            if (!CheckIdMatch(links, pattern.Index, link.Index, currentSolution))
            {
                yield break;
            }

            var sourceMatches = RecursiveMatchSubPattern(links, pattern.Source, link.Source, currentSolution);
            foreach (var sourceSolution in sourceMatches)
            {
                var targetMatches = RecursiveMatchSubPattern(links, pattern.Target, link.Target, sourceSolution);
                foreach (var targetSolution in targetMatches)
                {
                    var combined = new Dictionary<string, uint>(targetSolution);
                    AssignVariableIfNeeded(pattern.Index, link.Index, combined);
                    yield return combined;
                }
            }
        }

        private static bool CheckIdMatch(
            INamedTypesLinks<uint> links,
            string patternId,
            uint candidateId,
            Dictionary<string, uint> currentSolution)
        {
            if (string.IsNullOrEmpty(patternId)) return true;
            if (patternId == "*") return true;

            if (IsVariable(patternId))
            {
                if (currentSolution.TryGetValue(patternId, out var existingVal))
                {
                    return existingVal == candidateId;
                }
                return true;
            }

            uint parsed = links.Constants.Any;
            if (TryParseLinkId(patternId, links, ref parsed))
            {
                if (parsed == links.Constants.Any) return true;
                return parsed == candidateId;
            }
            return true;
        }

        private static void AssignVariableIfNeeded(string id, uint value, Dictionary<string, uint> assignments)
        {
            if (IsVariable(id))
            {
                assignments[id] = value;
            }
        }

        private static bool IsVariable(string identifier)
        {
            return !string.IsNullOrEmpty(identifier) && identifier.StartsWith("$");
        }

        private static uint ResolveId(
            INamedTypesLinks<uint> links,
            string identifier,
            Dictionary<string, uint> currentSolution)
        {
            var anyConstant = links.Constants.Any;
            if (string.IsNullOrEmpty(identifier)) return anyConstant;
            if (currentSolution.TryGetValue(identifier, out var value))
            {
                return value;
            }
            if (IsVariable(identifier))
            {
                return anyConstant;
            }
            uint parsedValue = anyConstant;
            if (TryParseLinkId(identifier, links, ref parsedValue))
            {
                return parsedValue;
            }
            return anyConstant;
        }

        private static bool DetermineIfSolutionIsNoOperation(
            Dictionary<string, uint> solution,
            List<Pattern> restrictions,
            List<Pattern> substitutions,
            INamedTypesLinks<uint> links)
        {
            var substitutedRestrictions = restrictions
                .Select(r => ApplySolutionToPattern(links, solution, r, isSubstitution: false))
                .Where(link => link != null)
                .Select(link => new DoubletLink(link!))
                .ToList();

            var substitutedSubstitutions = ApplySolutionToPatterns(links, solution, substitutions, isSubstitution: true);

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
            INamedTypesLinks<uint> links,
            Dictionary<string, uint> solution,
            List<Pattern> patterns)
        {
            var matchedLinks = new List<DoubletLink>();
            foreach (var pattern in patterns)
            {
                var applied = ApplySolutionToPattern(links, solution, pattern);
                if (applied != null)
                {
                    var matches = links.All(applied);
                    foreach (var match in matches)
                    {
                        matchedLinks.Add(new DoubletLink(match));
                    }
                }
            }
            return matchedLinks.Distinct().ToList();
        }

        private static DoubletLink? ApplySolutionToPattern(
            INamedTypesLinks<uint> links,
            Dictionary<string, uint> solution,
            Pattern? pattern,
            bool isSubstitution = false,
            HashSet<uint>? visitedIndexes = null)
        {
            if (pattern == null) return null;
            visitedIndexes ??= new HashSet<uint>();

            // Retrieve the ANY constant once for both leaf and composite cases
            var anyConstant = links.Constants.Any;

            if (pattern.IsLeaf)
            {
                uint resolvedIndex = ResolveId(links, pattern.Index, solution);
                return new DoubletLink(resolvedIndex, anyConstant, anyConstant);
            }
            else
            {
                uint resolvedIndex = ResolvePatternIndex(links, pattern.Index, solution, isSubstitution);
                var sourceLink = ApplySolutionToPattern(links, solution, pattern.Source, isSubstitution, visitedIndexes);
                var targetLink = ApplySolutionToPattern(links, solution, pattern.Target, isSubstitution, visitedIndexes);

                uint resolvedSource = sourceLink?.Index ?? anyConstant;
                uint resolvedTarget = targetLink?.Index ?? anyConstant;

                PreserveExistingSubstitutionParts(links, solution, pattern, resolvedIndex, ref resolvedSource, ref resolvedTarget, isSubstitution, visitedIndexes);

                if (resolvedSource == 0) resolvedSource = anyConstant;
                if (resolvedTarget == 0) resolvedTarget = anyConstant;

                return new DoubletLink(resolvedIndex, resolvedSource, resolvedTarget);
            }
        }

        private static uint ResolvePatternIndex(
            INamedTypesLinks<uint> links,
            string identifier,
            Dictionary<string, uint> solution,
            bool isSubstitution)
        {
            if (isSubstitution && string.IsNullOrEmpty(identifier))
            {
                return links.Constants.Null;
            }

            if (isSubstitution && IsVariable(identifier) && !solution.ContainsKey(identifier))
            {
                return links.Constants.Null;
            }

            return ResolveId(links, identifier, solution);
        }

        private static List<DoubletLink> ApplySolutionToPatterns(
            INamedTypesLinks<uint> links,
            Dictionary<string, uint> solution,
            List<Pattern> patterns,
            bool isSubstitution)
        {
            var workingSolution = isSubstitution ? new Dictionary<string, uint>(solution) : solution;
            return patterns
                .Select(pattern => ApplySolutionToPattern(links, workingSolution, pattern, isSubstitution))
                .Where(link => link != null)
                .Select(link => new DoubletLink(link!))
                .ToList();
        }

        private static void PreserveExistingSubstitutionParts(
            INamedTypesLinks<uint> links,
            Dictionary<string, uint> solution,
            Pattern pattern,
            uint resolvedIndex,
            ref uint resolvedSource,
            ref uint resolvedTarget,
            bool isSubstitution,
            HashSet<uint> visitedIndexes)
        {
            if (!isSubstitution || resolvedIndex == links.Constants.Null || resolvedIndex == links.Constants.Any || !links.Exists(resolvedIndex))
            {
                return;
            }

            if (!visitedIndexes.Add(resolvedIndex))
            {
                return;
            }

            try
            {
                var existingLink = new DoubletLink(links.GetLink(resolvedIndex));

                if (ShouldPreserveExistingPart(pattern.Source, solution) && CanPreserveExistingPart(existingLink, existingLink.Source, visitedIndexes))
                {
                    resolvedSource = existingLink.Source;
                    AssignVariableIfNeeded(pattern.Source!.Index, resolvedSource, solution);
                }
                else if (TryResolveVariablePart(pattern.Source, solution, out var boundSource))
                {
                    resolvedSource = boundSource;
                }

                if (ShouldPreserveExistingPart(pattern.Target, solution) && CanPreserveExistingPart(existingLink, existingLink.Target, visitedIndexes))
                {
                    resolvedTarget = existingLink.Target;
                    AssignVariableIfNeeded(pattern.Target!.Index, resolvedTarget, solution);
                }
                else if (TryResolveVariablePart(pattern.Target, solution, out var boundTarget))
                {
                    resolvedTarget = boundTarget;
                }
            }
            finally
            {
                visitedIndexes.Remove(resolvedIndex);
            }
        }

        private static bool ShouldPreserveExistingPart(Pattern? partPattern, Dictionary<string, uint> solution)
        {
            return partPattern?.IsLeaf == true
                && IsVariable(partPattern.Index)
                && !solution.ContainsKey(partPattern.Index);
        }

        private static bool TryResolveVariablePart(Pattern? partPattern, Dictionary<string, uint> solution, out uint value)
        {
            value = default;
            return partPattern?.IsLeaf == true
                && IsVariable(partPattern.Index)
                && solution.TryGetValue(partPattern.Index, out value);
        }

        private static bool CanPreserveExistingPart(DoubletLink existingLink, uint part, HashSet<uint> visitedIndexes)
        {
            return existingLink.IsFullPoint()
                || existingLink.IsPartialPoint()
                || !visitedIndexes.Contains(part);
        }
    }
}
