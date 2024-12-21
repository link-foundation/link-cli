using Platform.Delegates;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Protocols.Lino;
using System.Linq;
using System.Collections.Generic;
using System;
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

            if (outerLinkValues == null || outerLinkValues.Count < 2)
            {
                return;
            }

            var restrictionLink = outerLinkValues[0];
            var substitutionLink = outerLinkValues[1];

            // If both restriction and substitution are empty, do nothing
            if ((restrictionLink.Values?.Count == 0) && (substitutionLink.Values?.Count == 0))
            {
                return;
            }

            // Creation scenario: no restriction, only substitution
            if (restrictionLink.Values?.Count == 0 && (substitutionLink.Values?.Count ?? 0) > 0)
            {
                foreach (var linkToCreate in substitutionLink.Values ?? new List<LinoLink>())
                {
                    EnsureNestedLinkCreatedRecursively(links, linkToCreate, options);
                }
                return;
            }

            /*
             * We REMOVED the old "Deletion scenario" block that directly removed links
             * so that ANY restriction (including nested) is handled by pattern matching.
             */

            var restrictionPatterns = restrictionLink.Values ?? new List<LinoLink>();
            var substitutionPatterns = substitutionLink.Values ?? new List<LinoLink>();

            var restrictionInternalPatterns = restrictionPatterns.Select(l => CreatePatternFromLino(l)).ToList();
            var substitutionInternalPatterns = substitutionPatterns.Select(l => CreatePatternFromLino(l)).ToList();

            // If restrictionLink.Id is not empty, treat it as an extra pattern
            if (!string.IsNullOrEmpty(restrictionLink.Id))
            {
                var extraRestrictionPattern = CreatePatternFromLino(restrictionLink);
                restrictionInternalPatterns.Insert(0, extraRestrictionPattern);
            }

            // If substitutionLink.Id is not empty, treat it as an extra pattern
            if (!string.IsNullOrEmpty(substitutionLink.Id))
            {
                var extraSubstitutionPattern = CreatePatternFromLino(substitutionLink);
                substitutionInternalPatterns.Insert(0, extraSubstitutionPattern);
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
                    var substitutionLinks = substitutionInternalPatterns
                        .Select(pattern => ApplySolutionToPattern(links, solution, pattern))
                        .Where(link => link != null)
                        .Select(link => new DoubletLink(link!))
                        .ToList();
                    var restrictionLinks = restrictionInternalPatterns
                        .Select(pattern => ApplySolutionToPattern(links, solution, pattern))
                        .Where(link => link != null)
                        .Select(link => new DoubletLink(link!))
                        .ToList();

                    // Updated DetermineOperationsFromPatterns method
                    var operations = DetermineOperationsFromPatterns(restrictionLinks, substitutionLinks, links);
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

        /// <summary>
        /// Recursively ensures that a LinoLink (potentially nested at any depth) is created as a doublet link.
        /// Returns the ID of the created/ensured link if it's a composite link, or the numeric value if it's a leaf.
        /// </summary>
        private static uint EnsureNestedLinkCreatedRecursively(ILinks<uint> links, LinoLink lino, Options options)
        {
            var nullConstant = links.Constants.Null;
            var anyConstant = links.Constants.Any;

            if (lino.Values == null || lino.Values.Count == 0)
            {
                if (string.IsNullOrEmpty(lino.Id))
                {
                    return anyConstant;
                }
                if (lino.Id == "*")
                {
                    return anyConstant;
                }
                if (uint.TryParse(lino.Id, out uint parsedNumber))
                {
                    return parsedNumber;
                }
                return anyConstant;
            }

            if (lino.Values.Count == 2)
            {
                uint sourceId = EnsureNestedLinkCreatedRecursively(links, lino.Values[0], options);
                uint targetId = EnsureNestedLinkCreatedRecursively(links, lino.Values[1], options);

                uint index = 0;
                if (!string.IsNullOrEmpty(lino.Id))
                {
                    if (lino.Id == "*")
                    {
                        index = anyConstant;
                    }
                    else if (uint.TryParse(lino.Id.Replace(":", ""), out uint parsedIndex))
                    {
                        index = parsedIndex;
                    }
                }

                var linkToCreate = new DoubletLink(index, sourceId, targetId);
                return EnsureLinkCreated(links, linkToCreate, options);
            }

            return anyConstant;
        }

        private static void RestoreUnexpectedLinkDeletions(
            ILinks<uint> links,
            List<DoubletLink> unexpectedDeletions,
            Dictionary<uint, DoubletLink> finalIntendedStates,
            Options options)
        {
            if (unexpectedDeletions.Count > 0)
            {
                foreach (var deletedLink in unexpectedDeletions)
                {
                    if (finalIntendedStates.TryGetValue(deletedLink.Index, out var intendedLink))
                    {
                        if (intendedLink.Index == 0)
                        {
                            continue;
                        }
                        if (!links.Exists(intendedLink.Index))
                        {
                            CreateOrUpdateLink(links, intendedLink, options);
                        }
                    }
                }
            }
        }

        /// <summary>
        /// Updated method to avoid dictionary collisions with ANY/0 indexes.
        /// </summary>
        private static List<(DoubletLink before, DoubletLink after)> DetermineOperationsFromPatterns(
            List<DoubletLink> restrictions,
            List<DoubletLink> substitutions,
            ILinks<uint> links)
        {
            // We'll treat "0" or "Any" as "wildcard index" that can't be dictionary-keyed.

            // Separate normal vs. wildcard
            var anyOrZero = new HashSet<uint> { 0, links.Constants.Any };
            var normalRestrictions = restrictions.Where(r => !anyOrZero.Contains(r.Index)).ToList();
            var wildcardRestrictions = restrictions.Where(r => anyOrZero.Contains(r.Index)).ToList();

            var normalSubstitutions = substitutions.Where(s => !anyOrZero.Contains(s.Index)).ToList();
            var wildcardSubstitutions = substitutions.Where(s => anyOrZero.Contains(s.Index)).ToList();

            // Build dictionaries for normal (unique) indexes
            var restrictionByIndex = normalRestrictions.ToDictionary(r => r.Index, r => r);
            var substitutionByIndex = normalSubstitutions.ToDictionary(s => s.Index, s => s);

            var operations = new List<(DoubletLink before, DoubletLink after)>();

            // Step 1) Handle normal index => dictionary approach
            var allIndices = restrictionByIndex.Keys.Union(substitutionByIndex.Keys).ToList();
            foreach (var idx in allIndices)
            {
                bool hasRestriction = restrictionByIndex.TryGetValue(idx, out var rLink);
                bool hasSubstitution = substitutionByIndex.TryGetValue(idx, out var sLink);

                if (hasRestriction && hasSubstitution)
                {
                    // If the source/target differ => update, else => no-op
                    if (rLink.Source != sLink.Source || rLink.Target != sLink.Target)
                    {
                        operations.Add((rLink, sLink));
                    }
                    else
                    {
                        operations.Add((rLink, rLink));
                    }
                }
                else if (hasRestriction && !hasSubstitution)
                {
                    // Deletion
                    operations.Add((rLink, default(DoubletLink)));
                }
                else if (!hasRestriction && hasSubstitution)
                {
                    // Creation
                    operations.Add((default(DoubletLink), sLink));
                }
            }

            // Step 2) Wildcard restrictions => each is a separate "delete" operation
            foreach (var rLink in wildcardRestrictions)
            {
                operations.Add((rLink, default(DoubletLink)));
            }

            // Step 3) Wildcard substitutions => each is a separate "create" operation
            foreach (var sLink in wildcardSubstitutions)
            {
                operations.Add((default(DoubletLink), sLink));
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
                    // Deletion
                    RemoveLinks(links, before, options);
                }
                else if (before.Index == 0 && after.Index != 0)
                {
                    // Creation
                    CreateOrUpdateLink(links, after, options);
                }
                else if (before.Index != 0 && after.Index != 0)
                {
                    // Possible update
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

            for (int i = 0; i < patterns.Count; i++)
            {
                var pattern = patterns[i];
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
            if (pattern.IsLeaf)
            {
                uint indexValue = ResolveId(links, pattern.Index, currentSolution);
                uint sourceValue = links.Constants.Any;
                uint targetValue = links.Constants.Any;

                var candidates = links.All(new DoubletLink(indexValue, sourceValue, targetValue));
                foreach (var link in candidates)
                {
                    var candidateLink = new DoubletLink(link);
                    var assignments = new Dictionary<string, uint>();
                    AssignVariableIfNeeded(pattern.Index, candidateLink.Index, assignments);
                    yield return assignments;
                }
                yield break;
            }

            var any = links.Constants.Any;
            bool indexIsVariable = IsVariable(pattern.Index);
            bool indexIsAny = pattern.Index == "*";
            uint indexResolved = ResolveId(links, pattern.Index, currentSolution);

            if (!indexIsVariable && !indexIsAny && indexResolved != any && indexResolved != 0 && links.Exists(indexResolved))
            {
                var link = new DoubletLink(links.GetLink(indexResolved));
                foreach (var sourceSolution in RecursiveMatchSubPattern(links, pattern.Source, link.Source, currentSolution))
                {
                    foreach (var targetSolution in RecursiveMatchSubPattern(links, pattern.Target, link.Target, sourceSolution))
                    {
                        var combinedSolution = new Dictionary<string, uint>(targetSolution);
                        AssignVariableIfNeeded(pattern.Index, indexResolved, combinedSolution);
                        yield return combinedSolution;
                    }
                }
            }
            else
            {
                var allLinks = links.All(new DoubletLink(any, any, any));
                foreach (var raw in allLinks)
                {
                    var candidateLink = new DoubletLink(raw);
                    if (!CheckIdMatch(links, pattern.Index, candidateLink.Index, currentSolution))
                    {
                        continue;
                    }
                    foreach (var sourceSolution in RecursiveMatchSubPattern(links, pattern.Source, candidateLink.Source, currentSolution))
                    {
                        foreach (var targetSolution in RecursiveMatchSubPattern(links, pattern.Target, candidateLink.Target, sourceSolution))
                        {
                            var combinedSolution = new Dictionary<string, uint>(targetSolution);
                            AssignVariableIfNeeded(pattern.Index, candidateLink.Index, combinedSolution);
                            yield return combinedSolution;
                        }
                    }
                }
            }
        }

        private static IEnumerable<Dictionary<string, uint>> RecursiveMatchSubPattern(
            ILinks<uint> links,
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

            if (!links.Exists(linkId))
            {
                yield break;
            }

            var link = new DoubletLink(links.GetLink(linkId));
            if (!CheckIdMatch(links, pattern.Index, link.Index, currentSolution))
            {
                yield break;
            }

            foreach (var sourceSolution in RecursiveMatchSubPattern(links, pattern.Source, link.Source, currentSolution))
            {
                foreach (var targetSolution in RecursiveMatchSubPattern(links, pattern.Target, link.Target, sourceSolution))
                {
                    var combinedSolution = new Dictionary<string, uint>(targetSolution);
                    AssignVariableIfNeeded(pattern.Index, link.Index, combinedSolution);
                    yield return combinedSolution;
                }
            }
        }

        private static bool CheckIdMatch(ILinks<uint> links, string patternId, uint candidateId, Dictionary<string, uint> currentSolution)
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
            if (TryParseLinkId(patternId, links.Constants, ref parsed))
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
            var substitutedRestrictions = restrictions
                .Select(r => ApplySolutionToPattern(links, solution, r))
                .Where(link => link != null).Select(link => new DoubletLink(link!)).ToList();
            var substitutedSubstitutions = substitutions
                .Select(s => ApplySolutionToPattern(links, solution, s))
                .Where(link => link != null).Select(link => new DoubletLink(link!)).ToList();

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
                if (appliedPattern != null)
                {
                    var matches = links.All(appliedPattern);
                    foreach (var match in matches)
                    {
                        matchedLinks.Add(new DoubletLink(match));
                    }
                }
            }
            return matchedLinks.Distinct().ToList();
        }

        private static DoubletLink? ApplySolutionToPattern(
            ILinks<uint> links,
            Dictionary<string, uint> solution,
            Pattern? pattern)
        {
            if (pattern == null)
            {
                return null;
            }
            if (pattern.IsLeaf)
            {
                uint index = ResolveId(links, pattern.Index, solution);
                uint any = links.Constants.Any;
                return new DoubletLink(index, any, any);
            }
            else
            {
                uint index = ResolveId(links, pattern.Index, solution);
                var sourceLink = ApplySolutionToPattern(links, solution, pattern.Source);
                var targetLink = ApplySolutionToPattern(links, solution, pattern.Target);

                var any = links.Constants.Any;
                uint finalSource = sourceLink?.Index ?? any;
                uint finalTarget = targetLink?.Index ?? any;

                if (finalSource == 0) finalSource = any;
                if (finalTarget == 0) finalTarget = any;

                return new DoubletLink(index, finalSource, finalTarget);
            }
        }

        private static void CreateOrUpdateLink(ILinks<uint> links, DoubletLink link, Options options)
        {
            var nullConstant = links.Constants.Null;
            var anyConstant = links.Constants.Any;

            if (link.Index != nullConstant)
            {
                if (!links.Exists(link.Index))
                {
                    LinksExtensions.EnsureCreated(links, link.Index);
                }
                var existingLink = links.GetLink(link.Index);
                var existingDoublet = new DoubletLink(existingLink);
                if (existingDoublet.Source != link.Source || existingDoublet.Target != link.Target)
                {
                    LinksExtensions.EnsureCreated(links, link.Index);
                    options.ChangesHandler?.Invoke(new DoubletLink(link.Index, nullConstant, nullConstant), new DoubletLink(link.Index, nullConstant, nullConstant));
                    links.Update(new DoubletLink(link.Index, anyConstant, anyConstant), link, (b, a) =>
                        options.ChangesHandler?.Invoke(b, a) ?? links.Constants.Continue);
                }
                else
                {
                    options.ChangesHandler?.Invoke(existingDoublet, existingDoublet);
                }
            }
            else
            {
                // index=0 => create or retrieve
                uint createdIndex = links.SearchOrDefault(link.Source, link.Target);
                if (createdIndex == default)
                {
                    uint foundId = 0;
                    links.CreateAndUpdate(link.Source, link.Target, (before, after) =>
                    {
                        var afterLink = new DoubletLink(after);
                        if (foundId == 0 && afterLink.Index != 0 && afterLink.Index != anyConstant)
                        {
                            foundId = afterLink.Index;
                        }
                        return options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;
                    });
                    if (foundId == 0 || foundId == anyConstant)
                    {
                        foundId = links.SearchOrDefault(link.Source, link.Target);
                    }
                }
                else
                {
                    var existingLink = new DoubletLink(createdIndex, link.Source, link.Target);
                    options.ChangesHandler?.Invoke(existingLink, existingLink);
                }
            }
        }

        private static void RemoveLinks(ILinks<uint> links, DoubletLink restriction, Options options)
        {
            var linksToRemove = links.All(restriction).Where(l => l != null).Select(l => new DoubletLink(l)).ToList();
            foreach (var link in linksToRemove)
            {
                if (links.Exists(link.Index))
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
            else if (id.EndsWith(":"))
            {
                var trimmed = id.TrimEnd(':');
                if (uint.TryParse(trimmed, out uint linkId))
                {
                    parsedValue = linkId;
                    return true;
                }
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
            public Pattern? Source;
            public Pattern? Target;

            public Pattern(string index, Pattern? source = null, Pattern? target = null)
            {
                Index = index ?? "";
                Source = source;
                Target = target;
            }

            public bool IsLeaf => Source == null && Target == null;
        }

        private static Pattern CreatePatternFromLino(LinoLink lino)
        {
            if (lino.Values == null || lino.Values.Count == 0)
            {
                return new Pattern(lino.Id ?? "");
            }

            if (lino.Values.Count == 2)
            {
                var sourcePattern = CreatePatternFromLino(lino.Values[0]);
                var targetPattern = CreatePatternFromLino(lino.Values[1]);
                return new Pattern(lino.Id ?? "", sourcePattern, targetPattern);
            }

            return new Pattern(lino.Id ?? "");
        }

        private static uint EnsureLinkCreated(ILinks<uint> links, DoubletLink link, Options options)
        {
            var nullConstant = links.Constants.Null;
            var anyConstant = links.Constants.Any;

            if (link.Index == nullConstant)
            {
                var existingIndex = links.SearchOrDefault(link.Source, link.Target);
                if (existingIndex == default)
                {
                    uint createdIndex = 0;
                    links.CreateAndUpdate(link.Source, link.Target, (before, after) =>
                    {
                        var afterLink = new DoubletLink(after);
                        if (createdIndex == 0 && afterLink.Index != 0 && afterLink.Index != anyConstant)
                        {
                            createdIndex = afterLink.Index;
                        }
                        return options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;
                    });

                    if (createdIndex == 0 || createdIndex == anyConstant)
                    {
                        createdIndex = links.SearchOrDefault(link.Source, link.Target);
                    }
                    return createdIndex;
                }
                else
                {
                    var existingLink = new DoubletLink(existingIndex, link.Source, link.Target);
                    options.ChangesHandler?.Invoke(existingLink, existingLink);
                    return existingIndex;
                }
            }
            else
            {
                if (!links.Exists(link.Index))
                {
                    LinksExtensions.EnsureCreated(links, link.Index);
                }
                var existingLink = links.GetLink(link.Index);
                var existingDoublet = new DoubletLink(existingLink);
                if (existingDoublet.Source != link.Source || existingDoublet.Target != link.Target)
                {
                    uint finalIndex = link.Index;
                    links.Update(new DoubletLink(link.Index, links.Constants.Any, links.Constants.Any), link, (b, a) =>
                        options.ChangesHandler?.Invoke(b, a) ?? links.Constants.Continue);
                    return finalIndex;
                }
                else
                {
                    options.ChangesHandler?.Invoke(existingDoublet, existingDoublet);
                    return link.Index;
                }
            }
        }
    }
}