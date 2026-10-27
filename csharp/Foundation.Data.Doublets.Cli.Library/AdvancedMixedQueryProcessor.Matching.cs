using System.Numerics;
// Part of the partial AdvancedMixedQueryProcessor class.
// Pattern matching: turning restriction patterns into concrete variable solutions.
using System;
using Platform.Delegates;
using Platform.Data;
using Platform.Data.Doublets;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;
namespace Foundation.Data.Doublets.Cli
{
    public static partial class AdvancedMixedQueryProcessor<T>
        where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
    {
        private static List<Dictionary<string, T>> FindAllSolutions(INamedTypesLinks<T> links, List<Pattern> patterns)
        {
            var partialSolutions = new List<Dictionary<string, T>> { new Dictionary<string, T>() };

            for (int i = 0; i < patterns.Count; i++)
            {
                var pattern = patterns[i];
                var newSolutions = new List<Dictionary<string, T>>();
                foreach (var solution in partialSolutions)
                {
                    var matches = MatchPattern(links, pattern, solution).ToList();
                    foreach (var match in matches)
                    {
                        if (AreSolutionsCompatible(solution, match))
                        {
                            var combinedSolution = new Dictionary<string, T>(solution);
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
            Dictionary<string, T> existingSolution,
            Dictionary<string, T> newAssignments)
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

        private static IEnumerable<Dictionary<string, T>> MatchPattern(
            INamedTypesLinks<T> links,
            Pattern pattern,
            Dictionary<string, T> currentSolution)
        {
            var anyConstant = links.Constants.Any;
            if (pattern.IsLeaf)
            {
                T leafIndex = ResolveId(links, pattern.Index, currentSolution);
                var candidates = links.All(new LinkParts<T>(leafIndex, anyConstant, anyConstant));
                foreach (var link in candidates)
                {
                    var candidateLink = new LinkParts<T>(link);
                    var assignments = new Dictionary<string, T>();
                    AssignVariableIfNeeded(pattern.Index, candidateLink.Index, assignments);
                    yield return assignments;
                }
                yield break;
            }

            bool indexIsVariable = IsVariable(pattern.Index);
            bool indexIsAny = pattern.Index == "*";
            T resolvedIndex = ResolveId(links, pattern.Index, currentSolution);

            // If idxResolved is a known link => skip enumerating everything
            if (!indexIsVariable && !indexIsAny && resolvedIndex != anyConstant && resolvedIndex != T.Zero && links.Exists(resolvedIndex))
            {
                var link = new LinkParts<T>(links.GetLink(resolvedIndex));
                var sourceMatches = RecursiveMatchSubPattern(links, pattern.Source, link.Source, currentSolution);
                foreach (var sourceSolution in sourceMatches)
                {
                    var targetMatches = RecursiveMatchSubPattern(links, pattern.Target, link.Target, sourceSolution);
                    foreach (var targetSolution in targetMatches)
                    {
                        var combined = new Dictionary<string, T>(targetSolution);
                        AssignVariableIfNeeded(pattern.Index, resolvedIndex, combined);
                        yield return combined;
                    }
                }
            }
            else
            {
                // Otherwise we iterate over all links
                var allLinks = links.All(new LinkParts<T>(anyConstant, anyConstant, anyConstant));
                foreach (var raw in allLinks)
                {
                    var candidateLink = new LinkParts<T>(raw);
                    if (!CheckIdMatch(links, pattern.Index, candidateLink.Index, currentSolution))
                        continue;

                    var sourceMatches = RecursiveMatchSubPattern(links, pattern.Source, candidateLink.Source, currentSolution);
                    foreach (var sourceSolution in sourceMatches)
                    {
                        var targetMatches = RecursiveMatchSubPattern(links, pattern.Target, candidateLink.Target, sourceSolution);
                        foreach (var targetSolution in targetMatches)
                        {
                            var combined = new Dictionary<string, T>(targetSolution);
                            AssignVariableIfNeeded(pattern.Index, candidateLink.Index, combined);
                            yield return combined;
                        }
                    }
                }
            }
        }

        private static IEnumerable<Dictionary<string, T>> RecursiveMatchSubPattern(
            INamedTypesLinks<T> links,
            Pattern? pattern,
            T linkId,
            Dictionary<string, T> currentSolution)
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
                    var newSolution = new Dictionary<string, T>(currentSolution);
                    AssignVariableIfNeeded(pattern.Index, linkId, newSolution);
                    yield return newSolution;
                }
                yield break;
            }

            if (!links.Exists(linkId)) yield break;

            var link = new LinkParts<T>(links.GetLink(linkId));
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
                    var combined = new Dictionary<string, T>(targetSolution);
                    AssignVariableIfNeeded(pattern.Index, link.Index, combined);
                    yield return combined;
                }
            }
        }

        private static bool CheckIdMatch(
            INamedTypesLinks<T> links,
            string patternId,
            T candidateId,
            Dictionary<string, T> currentSolution)
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

            T parsed = links.Constants.Any;
            if (TryParseLinkId(patternId, links, ref parsed))
            {
                if (parsed == links.Constants.Any) return true;
                return parsed == candidateId;
            }
            return true;
        }

        private static void AssignVariableIfNeeded(string id, T value, Dictionary<string, T> assignments)
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

        private static T ResolveId(
            INamedTypesLinks<T> links,
            string identifier,
            Dictionary<string, T> currentSolution)
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
            T parsedValue = anyConstant;
            if (TryParseLinkId(identifier, links, ref parsedValue))
            {
                return parsedValue;
            }
            return anyConstant;
        }

        private static bool DetermineIfSolutionIsNoOperation(
            Dictionary<string, T> solution,
            List<Pattern> restrictions,
            List<Pattern> substitutions,
            INamedTypesLinks<T> links)
        {
            var substitutedRestrictions = restrictions
                .Select(r => ApplySolutionToPattern(links, solution, r, isSubstitution: false))
                .Where(link => link != null)
                .Select(link => new LinkParts<T>(link!))
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

        private static List<LinkParts<T>> ExtractMatchedLinks(
            INamedTypesLinks<T> links,
            Dictionary<string, T> solution,
            List<Pattern> patterns)
        {
            var matchedLinks = new List<LinkParts<T>>();
            foreach (var pattern in patterns)
            {
                var applied = ApplySolutionToPattern(links, solution, pattern);
                if (applied != null)
                {
                    var matches = links.All(applied);
                    foreach (var match in matches)
                    {
                        matchedLinks.Add(new LinkParts<T>(match));
                    }
                }
            }
            return matchedLinks.Distinct().ToList();
        }

        private static LinkParts<T>? ApplySolutionToPattern(
            INamedTypesLinks<T> links,
            Dictionary<string, T> solution,
            Pattern? pattern,
            bool isSubstitution = false,
            HashSet<T>? visitedIndexes = null)
        {
            if (pattern == null) return null;
            visitedIndexes ??= new HashSet<T>();

            // Retrieve the ANY constant once for both leaf and composite cases
            var anyConstant = links.Constants.Any;

            if (pattern.IsLeaf)
            {
                T resolvedIndex = ResolveId(links, pattern.Index, solution);
                return new LinkParts<T>(resolvedIndex, anyConstant, anyConstant);
            }
            else
            {
                T resolvedIndex = ResolvePatternIndex(links, pattern.Index, solution, isSubstitution);
                var sourceLink = ApplySolutionToPattern(links, solution, pattern.Source, isSubstitution, visitedIndexes);
                var targetLink = ApplySolutionToPattern(links, solution, pattern.Target, isSubstitution, visitedIndexes);

                T resolvedSource = sourceLink?.Index ?? anyConstant;
                T resolvedTarget = targetLink?.Index ?? anyConstant;

                PreserveExistingSubstitutionParts(links, solution, pattern, resolvedIndex, ref resolvedSource, ref resolvedTarget, isSubstitution, visitedIndexes);

                // Zero is a concrete bound reference. ResolveId already uses
                // Any for a missing variable; keep actual zero values intact.

                return new LinkParts<T>(resolvedIndex, resolvedSource, resolvedTarget);
            }
        }

        private static T ResolvePatternIndex(
            INamedTypesLinks<T> links,
            string identifier,
            Dictionary<string, T> solution,
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

        private static List<LinkParts<T>> ApplySolutionToPatterns(
            INamedTypesLinks<T> links,
            Dictionary<string, T> solution,
            List<Pattern> patterns,
            bool isSubstitution)
        {
            var workingSolution = isSubstitution ? new Dictionary<string, T>(solution) : solution;
            return patterns
                .Select(pattern => ApplySolutionToPattern(links, workingSolution, pattern, isSubstitution))
                .Where(link => link != null)
                .Select(link => new LinkParts<T>(link!))
                .ToList();
        }

        private static void PreserveExistingSubstitutionParts(
            INamedTypesLinks<T> links,
            Dictionary<string, T> solution,
            Pattern pattern,
            T resolvedIndex,
            ref T resolvedSource,
            ref T resolvedTarget,
            bool isSubstitution,
            HashSet<T> visitedIndexes)
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
                var existingLink = new LinkParts<T>(links.GetLink(resolvedIndex));

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

        private static bool ShouldPreserveExistingPart(Pattern? partPattern, Dictionary<string, T> solution)
        {
            return partPattern?.IsLeaf == true
                && IsVariable(partPattern.Index)
                && !solution.ContainsKey(partPattern.Index);
        }

        private static bool TryResolveVariablePart(Pattern? partPattern, Dictionary<string, T> solution, out T value)
        {
            value = default;
            return partPattern?.IsLeaf == true
                && IsVariable(partPattern.Index)
                && solution.TryGetValue(partPattern.Index, out value);
        }

        private static bool CanPreserveExistingPart(LinkParts<T> existingLink, T part, HashSet<T> visitedIndexes)
        {
            return existingLink.Index == existingLink.Source
                || existingLink.Index == existingLink.Target
                || !visitedIndexes.Contains(part);
        }
    }
}
