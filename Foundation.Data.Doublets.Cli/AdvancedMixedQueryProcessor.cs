using Platform.Delegates;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Protocols.Lino;
using System.Linq;
using System.Collections.Generic;
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
            if (outerLinkValues?.Count < 2)
            {
                return;
            }

            var restrictionLink = outerLinkValues![0];
            var substitutionLink = outerLinkValues![1];

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
                    // Recursively create the link, supports any nesting depth now
                    EnsureNestedLinkCreatedRecursively(links, linkToCreate, options);
                }
                return;
            }

            // Deletion scenario: no substitution, only restriction
            if (substitutionLink.Values?.Count == 0 && (restrictionLink.Values?.Count ?? 0) > 0)
            {
                var anyConstant = links.Constants.Any;
                foreach (var linkToDelete in restrictionLink.Values ?? new List<LinoLink>())
                {
                    var queryLink = ConvertToDoubletLink(links, linkToDelete, anyConstant);
                    RemoveLinks(links, queryLink, options);
                }
                return;
            }

            // Complex scenario (both restriction and substitution)
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
                // Leaf node: parse lino.Id as a number or '*' or return Any
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

            // If we have exactly 2 children, treat as (index?: source target)
            if (lino.Values.Count == 2)
            {
                uint sourceId = EnsureNestedLinkCreatedRecursively(links, lino.Values[0], options);
                uint targetId = EnsureNestedLinkCreatedRecursively(links, lino.Values[1], options);

                // Parse index if specified
                uint index = 0;
                if (!string.IsNullOrEmpty(lino.Id))
                {
                    if (lino.Id == "*")
                    {
                        index = anyConstant;
                    }
                    else if (uint.TryParse(lino.Id.Replace(":", ""), out uint parsedIndex))
                    {
                        // Handle cases like "2:" or just "2"
                        index = parsedIndex;
                    }
                }

                // Create or update link
                var linkToCreate = new DoubletLink(index, sourceId, targetId);
                return EnsureLinkCreated(links, linkToCreate, options);
            }

            // If not 2 values, fallback to Any
            return anyConstant;
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
                    options.ChangesHandler?.Invoke(null, new DoubletLink(link.Index, nullConstant, nullConstant));
                    links.Update(new DoubletLink(link.Index, anyConstant, anyConstant), link, (before, after) =>
                      options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue);
                }
                else
                {
                    options.ChangesHandler?.Invoke(existingDoublet, existingDoublet);
                }
            }
            else
            {
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
                        // If we did not get the ID from callback, search again
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
            else if (id.EndsWith(":"))
            {
                // If there's a trailing ':', remove it before parsing
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
            return new Pattern(index, source, target);
        }

        private static uint EnsureLinkCreated(ILinks<uint> links, DoubletLink link, Options options)
        {
            var nullConstant = links.Constants.Null;
            var anyConstant = links.Constants.Any;

            if (link.Index == nullConstant)
            {
                // Create if doesn't exist
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
                // If index is specified
                if (!links.Exists(link.Index))
                {
                    LinksExtensions.EnsureCreated(links, link.Index);
                }
                var existingLink = links.GetLink(link.Index);
                var existingDoublet = new DoubletLink(existingLink);
                if (existingDoublet.Source != link.Source || existingDoublet.Target != link.Target)
                {
                    uint finalIndex = link.Index;
                    links.Update(new DoubletLink(link.Index, anyConstant, anyConstant), link, (before, after) =>
                    {
                        var afterLink = new DoubletLink(after);
                        finalIndex = afterLink.Index;
                        return options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;
                    });
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