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
            Console.WriteLine($"[ProcessQuery] Query: {query}");
            if (string.IsNullOrEmpty(query))
            {
                Console.WriteLine("[ProcessQuery] Query is empty, returning...");
                return;
            }

            var parser = new Parser();
            var parsedLinks = parser.Parse(query);
            Console.WriteLine($"[ProcessQuery] Parsed {parsedLinks.Count} links.");
            if (parsedLinks.Count == 0)
            {
                Console.WriteLine("[ProcessQuery] No parsed links found, returning...");
                return;
            }

            var outerLink = parsedLinks[0];
            var outerLinkValues = outerLink.Values;
            if (outerLinkValues?.Count < 2)
            {
                Console.WriteLine("[ProcessQuery] Outer link values < 2, returning...");
                return;
            }

            var restrictionLink = outerLinkValues![0];
            var substitutionLink = outerLinkValues![1];

            Console.WriteLine($"[ProcessQuery] RestrictionLink: {restrictionLink.Id}, SubstitutionLink: {substitutionLink.Id}");
            Console.WriteLine($"[ProcessQuery] Restriction values count: {restrictionLink.Values?.Count}, Substitution values count: {substitutionLink.Values?.Count}");

            // If both restriction and substitution are empty, do nothing
            if ((restrictionLink.Values?.Count == 0) && (substitutionLink.Values?.Count == 0))
            {
                Console.WriteLine("[ProcessQuery] Both restriction and substitution empty, returning...");
                return;
            }

            // Creation scenario: no restriction, only substitution
            if (restrictionLink.Values?.Count == 0 && (substitutionLink.Values?.Count ?? 0) > 0)
            {
                Console.WriteLine("[ProcessQuery] Creation scenario detected.");
                foreach (var linkToCreate in substitutionLink.Values ?? new List<LinoLink>())
                {
                    Console.WriteLine($"[ProcessQuery] Ensuring creation of link: {linkToCreate.Id}");
                    EnsureNestedLinkCreatedRecursively(links, linkToCreate, options);
                }
                return;
            }

            // Deletion scenario: no substitution, only restriction
            if (substitutionLink.Values?.Count == 0 && (restrictionLink.Values?.Count ?? 0) > 0)
            {
                Console.WriteLine("[ProcessQuery] Deletion scenario detected.");
                var anyConstant = links.Constants.Any;
                foreach (var linkToDelete in restrictionLink.Values ?? new List<LinoLink>())
                {
                    Console.WriteLine($"[ProcessQuery] Deleting link: {linkToDelete.Id}");
                    var queryLink = ConvertToDoubletLink(links, linkToDelete, anyConstant);
                    RemoveLinks(links, queryLink, options);
                }
                return;
            }

            // Complex scenario (both restriction and substitution)
            Console.WriteLine("[ProcessQuery] Complex scenario detected.");
            var restrictionPatterns = restrictionLink.Values ?? new List<LinoLink>();
            var substitutionPatterns = substitutionLink.Values ?? new List<LinoLink>();

            Console.WriteLine("[ProcessQuery] Creating restriction patterns:");
            var restrictionInternalPatterns = restrictionPatterns.Select(l => CreatePatternFromLino(l)).ToList();
            foreach (var p in restrictionInternalPatterns) PrintPattern("RestrictionPattern", p);

            Console.WriteLine("[ProcessQuery] Creating substitution patterns:");
            var substitutionInternalPatterns = substitutionPatterns.Select(l => CreatePatternFromLino(l)).ToList();
            foreach (var p in substitutionInternalPatterns) PrintPattern("SubstitutionPattern", p);

            if (!string.IsNullOrEmpty(restrictionLink.Id))
            {
                var extraRestrictionPattern = CreatePatternFromLino(restrictionLink);
                PrintPattern("ExtraRestrictionPattern", extraRestrictionPattern);
                restrictionInternalPatterns.Insert(0, extraRestrictionPattern);
            }
            if (!string.IsNullOrEmpty(substitutionLink.Id))
            {
                var extraSubstitutionPattern = CreatePatternFromLino(substitutionLink);
                PrintPattern("ExtraSubstitutionPattern", extraSubstitutionPattern);
                substitutionInternalPatterns.Insert(0, extraSubstitutionPattern);
            }

            Console.WriteLine("[ProcessQuery] Final restriction patterns:");
            foreach (var p in restrictionInternalPatterns) PrintPattern("FinalRestrictionPattern", p);

            Console.WriteLine("[ProcessQuery] Final substitution patterns:");
            foreach (var p in substitutionInternalPatterns) PrintPattern("FinalSubstitutionPattern", p);

            var solutions = FindAllSolutions(links, restrictionInternalPatterns);
            Console.WriteLine($"[ProcessQuery] Solutions found: {solutions.Count}");
            for (int i = 0; i < solutions.Count; i++)
            {
                Console.WriteLine($"[ProcessQuery] Solution #{i}:");
                foreach (var kvp in solutions[i])
                {
                    Console.WriteLine($"  {kvp.Key} -> {kvp.Value}");
                }
            }

            if (solutions.Count == 0)
            {
                Console.WriteLine("[ProcessQuery] No solutions found, returning...");
                return;
            }

            bool allSolutionsNoOperation = solutions.All(solution =>
                DetermineIfSolutionIsNoOperation(solution, restrictionInternalPatterns, substitutionInternalPatterns, links));
            Console.WriteLine($"[ProcessQuery] AllSolutionsNoOperation: {allSolutionsNoOperation}");

            var allPlannedOperations = new List<(DoubletLink before, DoubletLink after)>();
            if (allSolutionsNoOperation)
            {
                Console.WriteLine("[ProcessQuery] All solutions result in no-operation, just confirming links.");
                foreach (var solution in solutions)
                {
                    var matchedLinks = ExtractMatchedLinks(links, solution, restrictionInternalPatterns);
                    Console.WriteLine($"[ProcessQuery] Matched links for no-op solution: {matchedLinks.Count}");
                    foreach (var link in matchedLinks)
                    {
                        Console.WriteLine($"  No-op Link: {link}");
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

                    Console.WriteLine("[ProcessQuery] Determining operations from patterns...");
                    var operations = DetermineOperationsFromPatterns(restrictionLinks, substitutionLinks);
                    foreach (var op in operations)
                    {
                        Console.WriteLine($"  Operation: Before={op.before} After={op.after}");
                    }
                    allPlannedOperations.AddRange(operations);
                }
            }

            Console.WriteLine("[ProcessQuery] All planned operations:");
            foreach (var op in allPlannedOperations)
            {
                Console.WriteLine($"  Before={op.before}, After={op.after}");
            }

            if (allSolutionsNoOperation)
            {
                foreach (var (before, after) in allPlannedOperations)
                {
                    Console.WriteLine($"[ProcessQuery] No-op change callback: Before={before}, After={after}");
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
                            Console.WriteLine($"[ProcessQuery] Unexpected deletion detected: {beforeLink}");
                            unexpectedDeletions.Add(new DoubletLink(beforeLink));
                        }
                    }
                    return originalHandler?.Invoke(before, after) ?? links.Constants.Continue;
                };
                Console.WriteLine("[ProcessQuery] Applying all planned operations...");
                ApplyAllPlannedOperations(links, allPlannedOperations, options);

                Console.WriteLine("[ProcessQuery] Restoring unexpected deletions if any...");
                RestoreUnexpectedLinkDeletions(links, unexpectedDeletions, intendedFinalStates, options);
            }
        }

        private static void PrintPattern(string label, Pattern pattern, int depth = 0)
        {
            string indent = new string(' ', depth * 2);
            Console.WriteLine($"{indent}{label}: Index={pattern.Index}, IsLeaf={pattern.IsLeaf}");
            if (!pattern.IsLeaf)
            {
                if (pattern.Source != null) PrintPattern(label + ".Source", pattern.Source, depth + 1);
                if (pattern.Target != null) PrintPattern(label + ".Target", pattern.Target, depth + 1);
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
                Console.WriteLine("[RestoreUnexpectedLinkDeletions] Attempting to restore unexpected deletions...");
            }

            var deletionsToProcess = new List<DoubletLink>(unexpectedDeletions);
            foreach (var deletedLink in deletionsToProcess)
            {
                Console.WriteLine($"[RestoreUnexpectedLinkDeletions] Checking {deletedLink}");
                if (finalIntendedStates.TryGetValue(deletedLink.Index, out var intendedLink))
                {
                    if (intendedLink.Index == 0) continue;
                    Console.WriteLine($"[RestoreUnexpectedLinkDeletions] Restoring {intendedLink}");
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
            Console.WriteLine("[ApplyAllPlannedOperations] Applying operations...");
            foreach (var (before, after) in operations)
            {
                Console.WriteLine($"[ApplyAllPlannedOperations] Operation: Before={before}, After={after}");
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
            Console.WriteLine("[FindAllSolutions] Starting...");
            foreach (var pattern in patterns)
            {
                PrintPattern("FindAllSolutions.Pattern", pattern);
                var newSolutions = new List<Dictionary<string, uint>>();
                foreach (var solution in partialSolutions)
                {
                    Console.WriteLine("[FindAllSolutions] Current partial solution:");
                    foreach (var kvp in solution) Console.WriteLine($"  {kvp.Key} -> {kvp.Value}");

                    foreach (var match in MatchPattern(links, pattern, solution))
                    {
                        Console.WriteLine("[FindAllSolutions] Found match:");
                        foreach (var kvp in match)
                        {
                            Console.WriteLine($"  {kvp.Key} -> {kvp.Value}");
                        }
                        if (AreSolutionsCompatible(solution, match))
                        {
                            var combinedSolution = new Dictionary<string, uint>(solution);
                            foreach (var assignment in match)
                            {
                                combinedSolution[assignment.Key] = assignment.Value;
                            }
                            Console.WriteLine("[FindAllSolutions] Combined compatible solution:");
                            foreach (var kvp in combinedSolution)
                            {
                                Console.WriteLine($"  {kvp.Key} -> {kvp.Value}");
                            }
                            newSolutions.Add(combinedSolution);
                        }
                        else
                        {
                            Console.WriteLine("[FindAllSolutions] Incompatible solution, skipping");
                        }
                    }
                }
                partialSolutions = newSolutions;
                Console.WriteLine($"[FindAllSolutions] After pattern, solutions count: {partialSolutions.Count}");
                if (partialSolutions.Count == 0)
                {
                    Console.WriteLine("[FindAllSolutions] No solutions left, stopping");
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
                        Console.WriteLine($"[AreSolutionsCompatible] Not compatible: {assignment.Key} existing={existingValue} new={assignment.Value}");
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
                Console.WriteLine($"[MatchPattern:Leaf] Pattern={pattern.Index}, Candidates={candidates.Count()}");
                foreach (var link in candidates)
                {
                    var candidateLink = new DoubletLink(link);
                    Console.WriteLine($"[MatchPattern:Leaf] Candidate={candidateLink}");
                    var assignments = new Dictionary<string, uint>();
                    AssignVariableIfNeeded(pattern.Index, candidateLink.Index, assignments);
                    foreach (var kvp in assignments)
                    {
                        Console.WriteLine($"[MatchPattern:Leaf] Assign {kvp.Key}={kvp.Value}");
                    }
                    yield return assignments;
                }
                yield break;
            }

            var any = links.Constants.Any;
            bool indexIsVariable = IsVariable(pattern.Index);
            bool indexIsAny = pattern.Index == "*";
            uint indexResolved = ResolveId(links, pattern.Index, currentSolution);
            Console.WriteLine($"[MatchPattern:Nested] Pattern Index={pattern.Index}, ResolvedIndex={indexResolved}");

            if (!indexIsVariable && !indexIsAny && indexResolved != any && indexResolved != 0 && links.Exists(indexResolved))
            {
                var link = new DoubletLink(links.GetLink(indexResolved));
                Console.WriteLine($"[MatchPattern:Nested] Exact link found: {link}");
                foreach (var sourceSolution in RecursiveMatchSubPattern(links, pattern.Source, link.Source, currentSolution))
                {
                    foreach (var targetSolution in RecursiveMatchSubPattern(links, pattern.Target, link.Target, sourceSolution))
                    {
                        var combinedSolution = new Dictionary<string, uint>(targetSolution);
                        AssignVariableIfNeeded(pattern.Index, indexResolved, combinedSolution);
                        Console.WriteLine("[MatchPattern:Nested] Matched exact index link with solutions:");
                        foreach (var kvp in combinedSolution)
                        {
                            Console.WriteLine($"  {kvp.Key} -> {kvp.Value}");
                        }
                        yield return combinedSolution;
                    }
                }
            }
            else
            {
                var allLinks = links.All(new DoubletLink(any, any, any));
                Console.WriteLine($"[MatchPattern:Nested] Trying all links. Count={allLinks.Count()}");
                foreach (var raw in allLinks)
                {
                    var candidateLink = new DoubletLink(raw);
                    if (!CheckIdMatch(links, pattern.Index, candidateLink.Index, currentSolution))
                    {
                        continue;
                    }
                    Console.WriteLine($"[MatchPattern:Nested] Candidate link: {candidateLink}");
                    foreach (var sourceSolution in RecursiveMatchSubPattern(links, pattern.Source, candidateLink.Source, currentSolution))
                    {
                        foreach (var targetSolution in RecursiveMatchSubPattern(links, pattern.Target, candidateLink.Target, sourceSolution))
                        {
                            var combinedSolution = new Dictionary<string, uint>(targetSolution);
                            AssignVariableIfNeeded(pattern.Index, candidateLink.Index, combinedSolution);
                            Console.WriteLine("[MatchPattern:Nested] Matched candidate link with solutions:");
                            foreach (var kvp in combinedSolution)
                            {
                                Console.WriteLine($"  {kvp.Key} -> {kvp.Value}");
                            }
                            yield return combinedSolution;
                        }
                    }
                }
            }
        }

        private static IEnumerable<Dictionary<string, uint>> RecursiveMatchSubPattern(
            ILinks<uint> links,
            Pattern pattern,
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
                    Console.WriteLine("[RecursiveMatchSubPattern:Leaf] Matched leaf pattern:");
                    foreach (var kvp in newSolution)
                    {
                        Console.WriteLine($"  {kvp.Key} -> {kvp.Value}");
                    }
                    yield return newSolution;
                }
                else
                {
                    Console.WriteLine("[RecursiveMatchSubPattern:Leaf] No match");
                }
                yield break;
            }

            if (!links.Exists(linkId))
            {
                Console.WriteLine("[RecursiveMatchSubPattern:Nested] linkId does not exist");
                yield break;
            }

            var link = new DoubletLink(links.GetLink(linkId));
            if (!CheckIdMatch(links, pattern.Index, link.Index, currentSolution))
            {
                Console.WriteLine("[RecursiveMatchSubPattern:Nested] Index does not match");
                yield break;
            }

            foreach (var sourceSolution in RecursiveMatchSubPattern(links, pattern.Source, link.Source, currentSolution))
            {
                foreach (var targetSolution in RecursiveMatchSubPattern(links, pattern.Target, link.Target, sourceSolution))
                {
                    var combinedSolution = new Dictionary<string, uint>(targetSolution);
                    AssignVariableIfNeeded(pattern.Index, link.Index, combinedSolution);
                    Console.WriteLine("[RecursiveMatchSubPattern:Nested] Matched nested pattern:");
                    foreach (var kvp in combinedSolution)
                    {
                        Console.WriteLine($"  {kvp.Key} -> {kvp.Value}");
                    }
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
                uint finalSource = sourceLink.Index == 0 ? any : sourceLink.Index;
                uint finalTarget = targetLink.Index == 0 ? any : targetLink.Index;
                if (finalSource == 0) finalSource = any;
                if (finalTarget == 0) finalTarget = any;

                return new DoubletLink(index, finalSource, finalTarget);
            }
        }

        private static void CreateOrUpdateLink(ILinks<uint> links, DoubletLink link, Options options)
        {
            var nullConstant = links.Constants.Null;
            var anyConstant = links.Constants.Any;

            Console.WriteLine($"[CreateOrUpdateLink] Before={link}");

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
            Console.WriteLine($"[RemoveLinks] Restriction={restriction}");
            var linksToRemove = links.All(restriction).ToList();
            Console.WriteLine($"[RemoveLinks] Found {linksToRemove.Count} links to remove.");
            foreach (var link in linksToRemove)
            {
                if (link != null && links.Exists(link[0]))
                {
                    Console.WriteLine($"[RemoveLinks] Removing link: {string.Join(",", link)}");
                    links.Delete(link, (before, after) =>
                      options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue);
                }
                else
                {
                    Console.WriteLine("[RemoveLinks] Link already not exists or null");
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
                return new Pattern(lino.Id);
            }

            if (lino.Values.Count == 2)
            {
                var sourcePattern = CreatePatternFromLino(lino.Values[0]);
                var targetPattern = CreatePatternFromLino(lino.Values[1]);
                return new Pattern(lino.Id, sourcePattern, targetPattern);
            }

            return new Pattern(lino.Id);
        }

        private static uint EnsureLinkCreated(ILinks<uint> links, DoubletLink link, Options options)
        {
            var nullConstant = links.Constants.Null;
            var anyConstant = links.Constants.Any;

            Console.WriteLine($"[EnsureLinkCreated] Link={link}");
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