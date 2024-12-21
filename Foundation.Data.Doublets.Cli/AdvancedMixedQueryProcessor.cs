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

            Console.WriteLine();
            Console.WriteLine("=== ProcessQuery START ===");
            Console.WriteLine($"Query: {query ?? "(null)"}");

            if (string.IsNullOrEmpty(query))
            {
                Console.WriteLine("Query is empty. Nothing to do.");
                Console.WriteLine("=== ProcessQuery END ===");
                return;
            }

            var parser = new Parser();
            var parsedLinks = parser.Parse(query);

            Console.WriteLine($"Parsed links count: {parsedLinks.Count}");
            if (parsedLinks.Count == 0)
            {
                Console.WriteLine("No parsed links from the query. Nothing to do.");
                Console.WriteLine("=== ProcessQuery END ===");
                return;
            }

            var outerLink = parsedLinks[0];
            var outerLinkValues = outerLink.Values;
            Console.WriteLine($"outerLink.Id = {outerLink.Id}, outerLink.Values?.Count = {outerLink.Values?.Count}");

            if (outerLinkValues?.Count < 2)
            {
                Console.WriteLine("Not enough elements (need 2) in the outer link Values. Nothing to do.");
                Console.WriteLine("=== ProcessQuery END ===");
                return;
            }

            var restrictionLink = outerLinkValues![0];
            var substitutionLink = outerLinkValues![1];

            Console.WriteLine($"restrictionLink.Id = {restrictionLink.Id}, restrictionLink.Values?.Count = {restrictionLink.Values?.Count}");
            Console.WriteLine($"substitutionLink.Id = {substitutionLink.Id}, substitutionLink.Values?.Count = {substitutionLink.Values?.Count}");

            // If both restriction and substitution are empty, do nothing
            if ((restrictionLink.Values?.Count == 0) && (substitutionLink.Values?.Count == 0))
            {
                Console.WriteLine("Both restriction and substitution are empty. Doing nothing.");
                Console.WriteLine("=== ProcessQuery END ===");
                return;
            }

            // Creation scenario: no restriction, only substitution
            if (restrictionLink.Values?.Count == 0 && (substitutionLink.Values?.Count ?? 0) > 0)
            {
                Console.WriteLine("Creation scenario triggered: no restriction, only substitution.");
                foreach (var linkToCreate in substitutionLink.Values ?? new List<LinoLink>())
                {
                    Console.WriteLine($"Ensuring nested link creation for substitution item: Id={linkToCreate.Id}, ValuesCount={linkToCreate.Values?.Count}");
                    EnsureNestedLinkCreatedRecursively(links, linkToCreate, options);
                }
                Console.WriteLine("=== ProcessQuery END ===");
                return;
            }

            /*
             * We REMOVED the old "Deletion scenario" block that directly removed links
             * so that ANY restriction (including nested) is handled by pattern matching.
             */

            Console.WriteLine("Complex scenario: using pattern matching (covers restriction-only or both).");

            var restrictionPatterns = restrictionLink.Values ?? new List<LinoLink>();
            var substitutionPatterns = substitutionLink.Values ?? new List<LinoLink>();

            var restrictionInternalPatterns = restrictionPatterns.Select(l => CreatePatternFromLino(l)).ToList();
            Console.WriteLine("Restriction patterns (from restrictionLink.Values):");
            foreach (var p in restrictionInternalPatterns) PrintPattern("RestrictionPattern", p);

            var substitutionInternalPatterns = substitutionPatterns.Select(l => CreatePatternFromLino(l)).ToList();
            Console.WriteLine("Substitution patterns (from substitutionLink.Values):");
            foreach (var p in substitutionInternalPatterns) PrintPattern("SubstitutionPattern", p);

            // If restrictionLink.Id is not empty, treat it as an extra pattern
            if (!string.IsNullOrEmpty(restrictionLink.Id))
            {
                var extraRestrictionPattern = CreatePatternFromLino(restrictionLink);
                Console.WriteLine("Extra restriction pattern (from restrictionLink.Id):");
                PrintPattern("ExtraRestrictionPattern", extraRestrictionPattern);
                restrictionInternalPatterns.Insert(0, extraRestrictionPattern);
            }

            // If substitutionLink.Id is not empty, treat it as an extra pattern
            if (!string.IsNullOrEmpty(substitutionLink.Id))
            {
                var extraSubstitutionPattern = CreatePatternFromLino(substitutionLink);
                Console.WriteLine("Extra substitution pattern (from substitutionLink.Id):");
                PrintPattern("ExtraSubstitutionPattern", extraSubstitutionPattern);
                substitutionInternalPatterns.Insert(0, extraSubstitutionPattern);
            }

            Console.WriteLine("Final restriction patterns:");
            foreach (var p in restrictionInternalPatterns) PrintPattern("FinalRestrictionPattern", p);

            Console.WriteLine("Final substitution patterns:");
            foreach (var p in substitutionInternalPatterns) PrintPattern("FinalSubstitutionPattern", p);

            var solutions = FindAllSolutions(links, restrictionInternalPatterns);
            Console.WriteLine($"Total solutions found: {solutions.Count}");

            if (solutions.Count == 0)
            {
                Console.WriteLine("No solutions matched the restriction patterns. Nothing to do.");
                Console.WriteLine("=== ProcessQuery END ===");
                return;
            }

            bool allSolutionsNoOperation = solutions.All(solution =>
                DetermineIfSolutionIsNoOperation(solution, restrictionInternalPatterns, substitutionInternalPatterns, links));

            Console.WriteLine($"allSolutionsNoOperation = {allSolutionsNoOperation}");

            var allPlannedOperations = new List<(DoubletLink before, DoubletLink after)>();
            if (allSolutionsNoOperation)
            {
                Console.WriteLine("All solutions produce 'no operation' scenario, just calling ChangesHandler for each matched link.");
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
                Console.WriteLine("At least one solution leads to actual changes (update, delete, or create).");
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

                    Console.WriteLine("[DEBUG] solution -> restrictionLinks:");
                    foreach (var rl in restrictionLinks)
                    {
                        Console.WriteLine($"    RL: index={rl.Index}, src={rl.Source}, trg={rl.Target}");
                    }
                    Console.WriteLine("[DEBUG] solution -> substitutionLinks:");
                    foreach (var sl in substitutionLinks)
                    {
                        Console.WriteLine($"    SL: index={sl.Index}, src={sl.Source}, trg={sl.Target}");
                    }

                    // Updated DetermineOperationsFromPatterns method
                    var operations = DetermineOperationsFromPatterns(restrictionLinks, substitutionLinks, links);
                    Console.WriteLine("[DEBUG] determined operations from patterns:");
                    foreach (var (b, a) in operations)
                    {
                        Console.WriteLine($"    BEFORE: [idx={b.Index}, src={b.Source}, trg={b.Target}], AFTER: [idx={a.Index}, src={a.Source}, trg={a.Target}]");
                    }
                    allPlannedOperations.AddRange(operations);
                }
            }

            Console.WriteLine($"Total planned operations: {allPlannedOperations.Count}");
            for (int i = 0; i < allPlannedOperations.Count; i++)
            {
                var (bef, aft) = allPlannedOperations[i];
                Console.WriteLine($"  OP[{i}]: BEFORE: idx={bef.Index}, src={bef.Source}, trg={bef.Target} | AFTER: idx={aft.Index}, src={aft.Source}, trg={aft.Target}");
            }

            if (allSolutionsNoOperation)
            {
                Console.WriteLine("No-op scenario: Just invoke ChangesHandler for each matched link pair (before, after) which are the same.");
                foreach (var (before, after) in allPlannedOperations)
                {
                    options.ChangesHandler?.Invoke(before, after);
                }
            }
            else
            {
                Console.WriteLine("Applying planned operations with a guard for unexpected deletions...");
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
                            Console.WriteLine($"[DEBUG] Unexpected deletion found for link index={beforeLink.Index}, src={beforeLink.Source}, trg={beforeLink.Target}.");
                            unexpectedDeletions.Add(new DoubletLink(beforeLink));
                        }
                    }
                    return originalHandler?.Invoke(before, after) ?? links.Constants.Continue;
                };
                ApplyAllPlannedOperations(links, allPlannedOperations, options);
                Console.WriteLine("Restoring any unexpected link deletions if needed...");
                RestoreUnexpectedLinkDeletions(links, unexpectedDeletions, intendedFinalStates, options);
            }

            Console.WriteLine("=== ProcessQuery END ===");
        }

        private static void PrintPattern(string label, Pattern pattern, int depth = 0)
        {
            // Simple line output
            Console.WriteLine($"{label}: Index='{pattern.Index}' (IsLeaf={pattern.IsLeaf})");
            if (!pattern.IsLeaf)
            {
                if (pattern.Source != null)
                {
                    Console.WriteLine($"  {label}.Source => {pattern.Source.Index}, IsLeaf={pattern.Source.IsLeaf}");
                }
                if (pattern.Target != null)
                {
                    Console.WriteLine($"  {label}.Target => {pattern.Target.Index}, IsLeaf={pattern.Target.IsLeaf}");
                }
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
                Console.WriteLine($"[DEBUG] EnsureNestedLinkCreatedRecursively -> index={index}, src={sourceId}, trg={targetId}");
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
            Console.WriteLine($"[DEBUG] RestoreUnexpectedLinkDeletions: unexpectedDeletions.Count={unexpectedDeletions.Count}");
            if (unexpectedDeletions.Count > 0)
            {
                Console.WriteLine("[DEBUG] We have unexpected deletions. Attempting to restore them if needed.");
            }

            var deletionsToProcess = new List<DoubletLink>(unexpectedDeletions);
            foreach (var deletedLink in deletionsToProcess)
            {
                Console.WriteLine($"[DEBUG] Checking intended final state for link index={deletedLink.Index}");
                if (finalIntendedStates.TryGetValue(deletedLink.Index, out var intendedLink))
                {
                    if (intendedLink.Index == 0)
                    {
                        Console.WriteLine($"[DEBUG] finalIntendedStates says index={deletedLink.Index} should remain deleted. No restore.");
                        continue;
                    }
                    if (!links.Exists(intendedLink.Index))
                    {
                        Console.WriteLine($"[DEBUG] Restoring link {intendedLink.Index}, src={intendedLink.Source}, trg={intendedLink.Target}");
                        CreateOrUpdateLink(links, intendedLink, options);
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
            Console.WriteLine("[DEBUG] DetermineOperationsFromPatterns => building dictionaries by Index, but skipping ANY collisions.");

            // We'll treat "0" or "Any" as "wildcard index" that can't be dictionary-keyed.
            var anyOrZero = new HashSet<uint> { 0, links.Constants.Any };

            Console.WriteLine("[DEBUG] Restriction links =>");
            foreach (var r in restrictions)
            {
                Console.WriteLine($"    R: idx={r.Index}, src={r.Source}, trg={r.Target}");
            }
            Console.WriteLine("[DEBUG] Substitution links =>");
            foreach (var s in substitutions)
            {
                Console.WriteLine($"    S: idx={s.Index}, src={s.Source}, trg={s.Target}");
            }

            // Separate normal vs. wildcard
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
            Console.WriteLine("[DEBUG] allIndices => " + string.Join(", ", allIndices));
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

            Console.WriteLine("[DEBUG] determined operations from patterns (with wildcard logic):");
            foreach (var (b,a) in operations)
            {
                Console.WriteLine($"    BEFORE: [idx={b.Index}, src={b.Source}, trg={b.Target}]  AFTER: [idx={a.Index}, src={a.Source}, trg={a.Target}]");
            }

            return operations;
        }

        private static void ApplyAllPlannedOperations(
            ILinks<uint> links,
            List<(DoubletLink before, DoubletLink after)> operations,
            Options options)
        {
            Console.WriteLine($"[DEBUG] ApplyAllPlannedOperations => count={operations.Count}");
            foreach (var (before, after) in operations)
            {
                if (before.Index != 0 && after.Index == 0)
                {
                    // Deletion
                    Console.WriteLine($"[DEBUG] Deleting link idx={before.Index}, src={before.Source}, trg={before.Target}");
                    RemoveLinks(links, before, options);
                }
                else if (before.Index == 0 && after.Index != 0)
                {
                    // Creation
                    Console.WriteLine($"[DEBUG] Creating link idx={after.Index}, src={after.Source}, trg={after.Target}");
                    CreateOrUpdateLink(links, after, options);
                }
                else if (before.Index != 0 && after.Index != 0)
                {
                    // Possible update
                    if (before.Source != after.Source || before.Target != after.Target)
                    {
                        if (before.Index == after.Index)
                        {
                            Console.WriteLine($"[DEBUG] Updating link in place idx={before.Index}, from (src={before.Source}, trg={before.Target}) to (src={after.Source}, trg={after.Target})");
                            if (!links.Exists(after.Index))
                            {
                                LinksExtensions.EnsureCreated(links, after.Index);
                            }
                            links.Update(before, after, (b, a) =>
                                options.ChangesHandler?.Invoke(b, a) ?? links.Constants.Continue);
                        }
                        else
                        {
                            Console.WriteLine($"[DEBUG] Deleting old link idx={before.Index}, then creating new link idx={after.Index}");
                            RemoveLinks(links, before, options);
                            CreateOrUpdateLink(links, after, options);
                        }
                    }
                    else
                    {
                        Console.WriteLine($"[DEBUG] No actual change. Invoking ChangesHandler for link idx={before.Index}");
                        options.ChangesHandler?.Invoke(before, before);
                    }
                }
            }
        }

        private static List<Dictionary<string, uint>> FindAllSolutions(ILinks<uint> links, List<Pattern> patterns)
        {
            Console.WriteLine("[DEBUG] FindAllSolutions =>");
            var partialSolutions = new List<Dictionary<string, uint>> { new Dictionary<string, uint>() };

            for (int i = 0; i < patterns.Count; i++)
            {
                var pattern = patterns[i];
                Console.WriteLine($"[DEBUG]  Matching pattern #{i}");
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
                Console.WriteLine($"[DEBUG]   partialSolutions after pattern #{i}: {partialSolutions.Count} solutions");
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
                    Console.WriteLine($"[DEBUG] CreateOrUpdateLink => ensure link with index={link.Index} is created in raw storage.");
                    LinksExtensions.EnsureCreated(links, link.Index);
                }
                var existingLink = links.GetLink(link.Index);
                var existingDoublet = new DoubletLink(existingLink);
                if (existingDoublet.Source != link.Source || existingDoublet.Target != link.Target)
                {
                    Console.WriteLine($"[DEBUG] Updating existing link idx={link.Index} to new src/trg={link.Source}/{link.Target}");
                    LinksExtensions.EnsureCreated(links, link.Index);
                    options.ChangesHandler?.Invoke(new DoubletLink(link.Index, nullConstant, nullConstant), new DoubletLink(link.Index, nullConstant, nullConstant));
                    links.Update(new DoubletLink(link.Index, anyConstant, anyConstant), link, (b, a) =>
                        options.ChangesHandler?.Invoke(b, a) ?? links.Constants.Continue);
                }
                else
                {
                    Console.WriteLine($"[DEBUG] Link idx={link.Index} is already (src={link.Source}, trg={link.Target}). Doing no-op ChangeHandler invoke.");
                    options.ChangesHandler?.Invoke(existingDoublet, existingDoublet);
                }
            }
            else
            {
                // index=0 => create or retrieve
                uint createdIndex = links.SearchOrDefault(link.Source, link.Target);
                if (createdIndex == default)
                {
                    Console.WriteLine($"[DEBUG] Creating a new link since none found for (src={link.Source}, trg={link.Target}).");
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
                    Console.WriteLine($"[DEBUG] Link (src={link.Source}, trg={link.Target}) already exists with idx={createdIndex}. Doing no-op ChangeHandler invoke.");
                    var existingLink = new DoubletLink(createdIndex, link.Source, link.Target);
                    options.ChangesHandler?.Invoke(existingLink, existingLink);
                }
            }
        }

        private static void RemoveLinks(ILinks<uint> links, DoubletLink restriction, Options options)
        {
            Console.WriteLine($"[DEBUG] RemoveLinks => restriction idx={restriction.Index}, src={restriction.Source}, trg={restriction.Target}");
            var linksToRemove = links.All(restriction).Where(l => l != null).Select(l => new DoubletLink(l)).ToList();
            Console.WriteLine($"[DEBUG] linksToRemove count: {linksToRemove.Count}");
            foreach (var link in linksToRemove)
            {
                Console.WriteLine($"[DEBUG] Attempting to delete link idx={link.Index}, src={link.Source}, trg={link.Target}");
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
                    links.Update(new DoubletLink(link.Index, links.Constants.Any, links.Constants.Any), link, (before, after) =>
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