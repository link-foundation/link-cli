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

            /// <summary>
            /// Enables extra console tracing of internal steps if true.
            /// </summary>
            public bool Trace { get; set; } = false;

            public static implicit operator Options(string query) => new Options { Query = query };
        }

        public static void ProcessQuery(ILinks<uint> links, Options options)
        {
            var query = options.Query;
            TraceIfEnabled(options, $"[ProcessQuery] Query: \"{query}\"");

            if (string.IsNullOrEmpty(query))
            {
                TraceIfEnabled(options, "[ProcessQuery] Query is empty, returning.");
                return;
            }

            var parser = new Parser();
            var parsedLinks = parser.Parse(query);

            TraceIfEnabled(options, $"[ProcessQuery] Parser returned {parsedLinks.Count} top-level link(s).");
            if (parsedLinks.Count == 0)
            {
                TraceIfEnabled(options, "[ProcessQuery] No top-level parsed links found, returning.");
                return;
            }

            var outerLink = parsedLinks[0];
            var outerLinkValues = outerLink.Values;
            if (outerLinkValues == null || outerLinkValues.Count < 2)
            {
                TraceIfEnabled(options, "[ProcessQuery] Outer link has fewer than 2 sub-links, returning.");
                return;
            }

            // Outer link is always "(( restriction ) ( substitution ))"
            var restrictionLink = outerLinkValues[0];
            var substitutionLink = outerLinkValues[1];
            TraceIfEnabled(options, $"[ProcessQuery] Restriction link => Id=\"{restrictionLink.Id}\" Values.Count={restrictionLink.Values?.Count ?? 0}");
            TraceIfEnabled(options, $"[ProcessQuery] Substitution link => Id=\"{substitutionLink.Id}\" Values.Count={substitutionLink.Values?.Count ?? 0}");

            // If both restriction and substitution are empty, do nothing
            if ((restrictionLink.Values?.Count == 0) && (substitutionLink.Values?.Count == 0))
            {
                TraceIfEnabled(options, "[ProcessQuery] Restriction & substitution both empty => no operation, returning.");
                return;
            }

            // Creation scenario: no restriction, only substitution
            if (restrictionLink.Values?.Count == 0 && (substitutionLink.Values?.Count ?? 0) > 0)
            {
                TraceIfEnabled(options, "[ProcessQuery] No restriction, but substitution is non-empty => creation scenario.");
                foreach (var linkToCreate in substitutionLink.Values ?? new List<LinoLink>())
                {
                    var createdId = EnsureNestedLinkCreatedRecursively(links, linkToCreate, options);
                    TraceIfEnabled(options, $"[ProcessQuery] Created link ID #{createdId} from substitution pattern.");
                }
                return;
            }

            // Next, we interpret the "restriction" and "substitution" links as pattern sets.
            // Each link can contain multiple LinoLinks as sub-values => each becomes a pattern.

            var restrictionPatterns = restrictionLink.Values ?? new List<LinoLink>();
            var substitutionPatterns = substitutionLink.Values ?? new List<LinoLink>();

            TraceIfEnabled(options, $"[ProcessQuery] Restriction patterns to parse: {restrictionPatterns.Count}");
            TraceIfEnabled(options, $"[ProcessQuery] Substitution patterns to parse: {substitutionPatterns.Count}");

            var restrictionInternalPatterns = restrictionPatterns.Select(l => CreatePatternFromLino(l)).ToList();
            var substitutionInternalPatterns = substitutionPatterns.Select(l => CreatePatternFromLino(l)).ToList();

            // If restrictionLink.Id is not empty, treat it as an extra pattern
            if (!string.IsNullOrEmpty(restrictionLink.Id))
            {
                TraceIfEnabled(options, "[ProcessQuery] Restriction link has non-empty Id => adding extra pattern for it.");
                var extraRestrictionPattern = CreatePatternFromLino(restrictionLink);
                restrictionInternalPatterns.Insert(0, extraRestrictionPattern);
            }

            // If substitutionLink.Id is not empty, treat it as an extra pattern
            if (!string.IsNullOrEmpty(substitutionLink.Id))
            {
                TraceIfEnabled(options, "[ProcessQuery] Substitution link has non-empty Id => adding extra pattern for it.");
                var extraSubstitutionPattern = CreatePatternFromLino(substitutionLink);
                substitutionInternalPatterns.Insert(0, extraSubstitutionPattern);
            }

            TraceIfEnabled(options, "[ProcessQuery] Converting restriction patterns => done.");
            TraceIfEnabled(options, "[ProcessQuery] Converting substitution patterns => done.");

            // Now find all solutions (assignments) that match those patterns.
            TraceIfEnabled(options, "[ProcessQuery] Finding solutions for restriction patterns...");
            var solutions = FindAllSolutions(links, restrictionInternalPatterns);

            TraceIfEnabled(options, $"[ProcessQuery] Found {solutions.Count} total solution(s) matching restriction patterns.");

            if (solutions.Count == 0)
            {
                TraceIfEnabled(options, "[ProcessQuery] No solutions found => returning.");
                return;
            }

            // Decide if all solutions would lead to a no-op.
            bool allSolutionsNoOperation = solutions.All(solution =>
                DetermineIfSolutionIsNoOperation(solution, restrictionInternalPatterns, substitutionInternalPatterns, links));

            TraceIfEnabled(options, "[ProcessQuery] allSolutionsNoOperation=" + allSolutionsNoOperation);

            var allPlannedOperations = new List<(DoubletLink before, DoubletLink after)>();
            if (allSolutionsNoOperation)
            {
                TraceIfEnabled(options, "[ProcessQuery] All solutions produce no differences => we'll track them as no-op changes.");
                foreach (var solution in solutions)
                {
                    var matchedLinks = ExtractMatchedLinks(links, solution, restrictionInternalPatterns);
                    TraceIfEnabled(options, $"[ProcessQuery] One solution => matched {matchedLinks.Count} link(s).");
                    foreach (var link in matchedLinks)
                    {
                        allPlannedOperations.Add((link, link));
                    }
                }
            }
            else
            {
                TraceIfEnabled(options, "[ProcessQuery] Some solutions lead to actual changes => building operations.");
                foreach (var solution in solutions)
                {
                    // For each solution, we apply it to the substitution patterns => get desired final state links
                    var substitutionLinks = substitutionInternalPatterns
                        .Select(pattern => ApplySolutionToPattern(links, solution, pattern))
                        .Where(link => link != null)
                        .Select(link => new DoubletLink(link!))
                        .ToList();

                    // Same with restriction patterns => those represent the "before" links
                    var restrictionLinks = restrictionInternalPatterns
                        .Select(pattern => ApplySolutionToPattern(links, solution, pattern))
                        .Where(link => link != null)
                        .Select(link => new DoubletLink(link!))
                        .ToList();

                    TraceIfEnabled(options,
                        "[ProcessQuery] For a solution => " +
                        $"substitution links count={substitutionLinks.Count}, restriction links count={restrictionLinks.Count}.");

                    // The DetermineOperationsFromPatterns method figures out creation/update/deletion steps
                    var operations = DetermineOperationsFromPatterns(restrictionLinks, substitutionLinks, links);
                    TraceIfEnabled(options, $"[ProcessQuery] => {operations.Count} operation(s) derived from these patterns.");
                    allPlannedOperations.AddRange(operations);
                }
            }

            TraceIfEnabled(options, "[ProcessQuery] All planned operations => " + allPlannedOperations.Count);

            if (allSolutionsNoOperation)
            {
                TraceIfEnabled(options, "[ProcessQuery] Since they're all no-ops, just calling ChangesHandler with (before, before).");
                foreach (var (before, after) in allPlannedOperations)
                {
                    options.ChangesHandler?.Invoke(before, after);
                }
            }
            else
            {
                // We track "intended final states" for each link ID
                var intendedFinalStates = new Dictionary<uint, DoubletLink>();
                foreach (var (before, after) in allPlannedOperations)
                {
                    if (after.Index != 0)
                    {
                        // Means an update or creation => final state is "after"
                        intendedFinalStates[after.Index] = after;
                    }
                    else if (before.Index != 0 && after.Index == 0)
                    {
                        // Means a deletion => final state is "deleted"
                        intendedFinalStates[before.Index] = default(DoubletLink);
                    }
                }

                // If some link is being unexpectedly deleted during an update, we’ll restore it afterward
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
                            TraceIfEnabled(options, $"[ProcessQuery] Detected unexpected deletion of link #{beforeLink.Index} => will restore later.");
                        }
                    }
                    return originalHandler?.Invoke(before, after) ?? links.Constants.Continue;
                };

                // Actually apply the operations (create, update, delete, etc.)
                TraceIfEnabled(options, "[ProcessQuery] Applying all planned operations...");
                ApplyAllPlannedOperations(links, allPlannedOperations, options);

                // Then restore anything that got unexpectedly deleted
                TraceIfEnabled(options, "[ProcessQuery] Restoring unexpected deletions if any...");
                RestoreUnexpectedLinkDeletions(links, unexpectedDeletions, intendedFinalStates, options);
            }

            TraceIfEnabled(options, "[ProcessQuery] Finished processing query.");
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
                    TraceIfEnabled(options, "[EnsureNestedLinkCreatedRecursively] Leaf with empty ID => returning ANY.");
                    return anyConstant;
                }
                if (lino.Id == "*")
                {
                    TraceIfEnabled(options, "[EnsureNestedLinkCreatedRecursively] Leaf with '*' => returning ANY.");
                    return anyConstant;
                }
                if (uint.TryParse(lino.Id, out uint parsedNumber))
                {
                    TraceIfEnabled(options, $"[EnsureNestedLinkCreatedRecursively] Leaf parse => returning {parsedNumber}.");
                    return parsedNumber;
                }
                TraceIfEnabled(options, "[EnsureNestedLinkCreatedRecursively] Leaf with unparseable => returning ANY.");
                return anyConstant;
            }

            // If we have exactly 2 Values => we interpret as a composite link
            if (lino.Values.Count == 2)
            {
                var sourceId = EnsureNestedLinkCreatedRecursively(links, lino.Values[0], options);
                var targetId = EnsureNestedLinkCreatedRecursively(links, lino.Values[1], options);

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
                var createdId = EnsureLinkCreated(links, linkToCreate, options);
                TraceIfEnabled(options,
                    $"[EnsureNestedLinkCreatedRecursively] Created/ensured composite link => (index={index}, src={sourceId}, trg={targetId}) => actual ID={createdId}");
                return createdId;
            }

            // If more than 2 sub-values, we do nothing special => or treat them as ANY
            TraceIfEnabled(options, "[EnsureNestedLinkCreatedRecursively] More than 2 sub-values => returning ANY.");
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
                TraceIfEnabled(options, $"[RestoreUnexpectedLinkDeletions] We have {unexpectedDeletions.Count} unexpected deletion(s) to handle.");
                foreach (var deletedLink in unexpectedDeletions)
                {
                    if (finalIntendedStates.TryGetValue(deletedLink.Index, out var intendedLink))
                    {
                        if (intendedLink.Index == 0)
                        {
                            TraceIfEnabled(options, $"[RestoreUnexpectedLinkDeletions] Link #{deletedLink.Index} was intended to be deleted, skipping.");
                            continue;
                        }
                        if (!links.Exists(intendedLink.Index))
                        {
                            TraceIfEnabled(options, $"[RestoreUnexpectedLinkDeletions] Link #{deletedLink.Index} is missing but intended => recreating.");
                            CreateOrUpdateLink(links, intendedLink, options);
                        }
                    }
                }
            }
            else
            {
                TraceIfEnabled(options, "[RestoreUnexpectedLinkDeletions] No unexpected deletions found.");
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
            var anyOrZero = new HashSet<uint> { 0, links.Constants.Any };

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
                    TraceIfEnabled(options, $"[ApplyAllPlannedOperations] Deleting link => ID={before.Index}, S={before.Source}, T={before.Target}");
                    RemoveLinks(links, before, options);
                }
                else if (before.Index == 0 && after.Index != 0)
                {
                    // Creation
                    TraceIfEnabled(options, $"[ApplyAllPlannedOperations] Creating link => ID={after.Index}, S={after.Source}, T={after.Target}");
                    CreateOrUpdateLink(links, after, options);
                }
                else if (before.Index != 0 && after.Index != 0)
                {
                    // Possible update
                    if (before.Source != after.Source || before.Target != after.Target)
                    {
                        // If it's the same Index, we do an Update; else remove + create
                        if (before.Index == after.Index)
                        {
                            TraceIfEnabled(options, $"[ApplyAllPlannedOperations] Updating link in-place => ID={before.Index}");
                            if (!links.Exists(after.Index))
                            {
                                LinksExtensions.EnsureCreated(links, after.Index);
                            }
                            links.Update(before, after, (b, a) =>
                                options.ChangesHandler?.Invoke(b, a) ?? links.Constants.Continue);
                        }
                        else
                        {
                            TraceIfEnabled(options, $"[ApplyAllPlannedOperations] Removing old link => ID={before.Index} then creating new => ID={after.Index}.");
                            RemoveLinks(links, before, options);
                            CreateOrUpdateLink(links, after, options);
                        }
                    }
                    else
                    {
                        // Source & target unchanged => no-op but we still trace
                        TraceIfEnabled(options, $"[ApplyAllPlannedOperations] No changes for link => ID={before.Index}, same source/target => no-op.");
                        options.ChangesHandler?.Invoke(before, before);
                    }
                }
            }
        }

        private static List<Dictionary<string, uint>> FindAllSolutions(ILinks<uint> links, List<Pattern> patterns)
        {
            var partialSolutions = new List<Dictionary<string, uint>> { new Dictionary<string, uint>() };

            // For each pattern, we try to match it, expand solutions...
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
                if (partialSolutions.Count == 0)
                {
                    break;
                }
            }

            return partialSolutions;
        }

        private static bool AreSolutionsCompatible(Dictionary<string, uint> existingSolution, Dictionary<string, uint> newAssignments)
        {
            // If the same variable has different assigned values => conflict => false
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
            // If pattern is a leaf => match index only
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

            // Non-leaf => we also need to match the source & target
            var any = links.Constants.Any;
            bool indexIsVariable = IsVariable(pattern.Index);
            bool indexIsAny = pattern.Index == "*";
            uint indexResolved = ResolveId(links, pattern.Index, currentSolution);

            // If indexResolved is a known link => we can skip enumerating all links
            if (!indexIsVariable && !indexIsAny && indexResolved != any && indexResolved != 0 && links.Exists(indexResolved))
            {
                var link = new DoubletLink(links.GetLink(indexResolved));
                // We check if link's Source & Target match the sub-patterns:
                var sourceMatches = RecursiveMatchSubPattern(links, pattern.Source, link.Source, currentSolution);
                foreach (var sourceSolution in sourceMatches)
                {
                    var targetMatches = RecursiveMatchSubPattern(links, pattern.Target, link.Target, sourceSolution);
                    foreach (var targetSolution in targetMatches)
                    {
                        var combinedSolution = new Dictionary<string, uint>(targetSolution);
                        AssignVariableIfNeeded(pattern.Index, indexResolved, combinedSolution);
                        yield return combinedSolution;
                    }
                }
            }
            else
            {
                // Otherwise we iterate over all links
                var allLinks = links.All(new DoubletLink(any, any, any));
                foreach (var raw in allLinks)
                {
                    var candidateLink = new DoubletLink(raw);
                    if (!CheckIdMatch(links, pattern.Index, candidateLink.Index, currentSolution))
                    {
                        continue;
                    }
                    // Then see if candidateLink.Source matches pattern.Source, candidateLink.Target matches pattern.Target
                    var sourceMatches = RecursiveMatchSubPattern(links, pattern.Source, candidateLink.Source, currentSolution);
                    foreach (var sourceSolution in sourceMatches)
                    {
                        var targetMatches = RecursiveMatchSubPattern(links, pattern.Target, candidateLink.Target, sourceSolution);
                        foreach (var targetSolution in targetMatches)
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
                // Null pattern => automatically a match
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

            // If pattern is not leaf, we retrieve linkId, see if it matches the pattern's own index, etc.
            if (!links.Exists(linkId))
            {
                yield break;
            }

            var link = new DoubletLink(links.GetLink(linkId));
            if (!CheckIdMatch(links, pattern.Index, link.Index, currentSolution))
            {
                yield break;
            }

            // Recurse on the source & target
            var sourceMatches = RecursiveMatchSubPattern(links, pattern.Source, link.Source, currentSolution);
            foreach (var sourceSolution in sourceMatches)
            {
                var targetMatches = RecursiveMatchSubPattern(links, pattern.Target, link.Target, sourceSolution);
                foreach (var targetSolution in targetMatches)
                {
                    var combinedSolution = new Dictionary<string, uint>(targetSolution);
                    AssignVariableIfNeeded(pattern.Index, link.Index, combinedSolution);
                    yield return combinedSolution;
                }
            }
        }

        private static bool CheckIdMatch(ILinks<uint> links, string patternId, uint candidateId, Dictionary<string, uint> currentSolution)
        {
            if (string.IsNullOrEmpty(patternId)) return true;        // no restriction => always match
            if (patternId == "*") return true;                      // wildcard => always match

            if (IsVariable(patternId))
            {
                // If we already have an assignment for that variable => must unify
                if (currentSolution.TryGetValue(patternId, out var existingVal))
                {
                    return existingVal == candidateId;
                }
                return true;
            }

            // Otherwise patternId might be a numeric or "Any"
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
            // We'll apply the solution to restriction & substitution => see if they match exactly
            var substitutedRestrictions = restrictions
                .Select(r => ApplySolutionToPattern(links, solution, r))
                .Where(link => link != null).Select(link => new DoubletLink(link!)).ToList();
            var substitutedSubstitutions = substitutions
                .Select(s => ApplySolutionToPattern(links, solution, s))
                .Where(link => link != null).Select(link => new DoubletLink(link!)).ToList();

            // Sort by index for a straightforward 1-1 check
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
            // For each pattern, apply the solution => get the "link form" => then see which actual links match it in the store
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
                // We have a desired index => if doesn't exist, ensure it's created, else update
                if (!links.Exists(link.Index))
                {
                    TraceIfEnabled(options, $"[CreateOrUpdateLink] Link #{link.Index} does not exist => ensuring creation.");
                    LinksExtensions.EnsureCreated(links, link.Index);
                }
                var existingLink = links.GetLink(link.Index);
                var existingDoublet = new DoubletLink(existingLink);

                if (existingDoublet.Source != link.Source || existingDoublet.Target != link.Target)
                {
                    // We do an update
                    TraceIfEnabled(options,
                        $"[CreateOrUpdateLink] Updating link #{link.Index} => Source: {existingDoublet.Source}->{link.Source}, Target: {existingDoublet.Target}->{link.Target}");
                    LinksExtensions.EnsureCreated(links, link.Index);
                    options.ChangesHandler?.Invoke(new DoubletLink(link.Index, nullConstant, nullConstant),
                                                   new DoubletLink(link.Index, nullConstant, nullConstant));
                    links.Update(new DoubletLink(link.Index, anyConstant, anyConstant), link, (b, a) =>
                        options.ChangesHandler?.Invoke(b, a) ?? links.Constants.Continue);
                }
                else
                {
                    // No change
                    TraceIfEnabled(options, $"[CreateOrUpdateLink] Link #{link.Index} already has S={link.Source}, T={link.Target} => no-op.");
                    options.ChangesHandler?.Invoke(existingDoublet, existingDoublet);
                }
            }
            else
            {
                // index=0 => create or retrieve
                var found = links.SearchOrDefault(link.Source, link.Target);
                if (found == default)
                {
                    uint newCreatedId = 0;
                    TraceIfEnabled(options, $"[CreateOrUpdateLink] No link found for (S={link.Source}, T={link.Target}), creating new link...");
                    links.CreateAndUpdate(link.Source, link.Target, (before, after) =>
                    {
                        var afterLink = new DoubletLink(after);
                        if (newCreatedId == 0 && afterLink.Index != 0 && afterLink.Index != anyConstant)
                        {
                            newCreatedId = afterLink.Index;
                            TraceIfEnabled(options, $"[CreateOrUpdateLink] Created new link => ID={newCreatedId}");
                        }
                        return options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;
                    });
                    if (newCreatedId == 0 || newCreatedId == anyConstant)
                    {
                        newCreatedId = links.SearchOrDefault(link.Source, link.Target);
                    }
                }
                else
                {
                    // Already exists => no-op
                    var existingLink = new DoubletLink(found, link.Source, link.Target);
                    TraceIfEnabled(options, $"[CreateOrUpdateLink] Link already exists => ID={found}, no changes needed.");
                    options.ChangesHandler?.Invoke(existingLink, existingLink);
                }
            }
        }

        private static void RemoveLinks(ILinks<uint> links, DoubletLink restriction, Options options)
        {
            var linksToRemove = links.All(restriction).Where(l => l != null).Select(l => new DoubletLink(l)).ToList();
            TraceIfEnabled(options, $"[RemoveLinks] Found {linksToRemove.Count} link(s) matching (ID={restriction.Index}, S={restriction.Source}, T={restriction.Target}).");
            foreach (var link in linksToRemove)
            {
                if (links.Exists(link.Index))
                {
                    TraceIfEnabled(options, $"[RemoveLinks] Deleting link => ID={link.Index}, S={link.Source}, T={link.Target}");
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
            // If there's no sub-values => leaf pattern
            if (lino.Values == null || lino.Values.Count == 0)
            {
                return new Pattern(lino.Id ?? "");
            }

            // If exactly 2 => treat as "index, source, target" triple
            if (lino.Values.Count == 2)
            {
                var sourcePattern = CreatePatternFromLino(lino.Values[0]);
                var targetPattern = CreatePatternFromLino(lino.Values[1]);
                return new Pattern(lino.Id ?? "", sourcePattern, targetPattern);
            }

            // If more than 2 => treat similarly to a leaf with an ID
            return new Pattern(lino.Id ?? "");
        }

        private static uint EnsureLinkCreated(ILinks<uint> links, DoubletLink link, Options options)
        {
            var nullConstant = links.Constants.Null;
            var anyConstant = links.Constants.Any;

            if (link.Index == nullConstant)
            {
                // If no index => search or create
                var existingIndex = links.SearchOrDefault(link.Source, link.Target);
                if (existingIndex == default)
                {
                    uint createdIndex = 0;
                    TraceIfEnabled(options, $"[EnsureLinkCreated] Creating new link for (S={link.Source}, T={link.Target}).");
                    links.CreateAndUpdate(link.Source, link.Target, (before, after) =>
                    {
                        var afterLink = new DoubletLink(after);
                        if (createdIndex == 0 && afterLink.Index != 0 && afterLink.Index != anyConstant)
                        {
                            createdIndex = afterLink.Index;
                            TraceIfEnabled(options, $"[EnsureLinkCreated] Assigned new ID => {createdIndex}");
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
                    // Already exists => no-op
                    var existingLink = new DoubletLink(existingIndex, link.Source, link.Target);
                    TraceIfEnabled(options, $"[EnsureLinkCreated] Link for (S={link.Source}, T={link.Target}) already found => ID={existingIndex}.");
                    options.ChangesHandler?.Invoke(existingLink, existingLink);
                    return existingIndex;
                }
            }
            else
            {
                // We have an index => ensure it is created, or update if needed
                if (!links.Exists(link.Index))
                {
                    TraceIfEnabled(options, $"[EnsureLinkCreated] Link ID={link.Index} doesn't exist => ensuring creation.");
                    LinksExtensions.EnsureCreated(links, link.Index);
                }
                var existingLink = links.GetLink(link.Index);
                var existingDoublet = new DoubletLink(existingLink);
                if (existingDoublet.Source != link.Source || existingDoublet.Target != link.Target)
                {
                    TraceIfEnabled(options,
                        $"[EnsureLinkCreated] Updating link #{link.Index} => Source={existingDoublet.Source}->{link.Source}, Target={existingDoublet.Target}->{link.Target}.");
                    uint finalIndex = link.Index;
                    links.Update(new DoubletLink(link.Index, links.Constants.Any, links.Constants.Any), link, (b, a) =>
                        options.ChangesHandler?.Invoke(b, a) ?? links.Constants.Continue);
                    return finalIndex;
                }
                else
                {
                    // No change
                    TraceIfEnabled(options, $"[EnsureLinkCreated] Link #{link.Index} already correct => no-op.");
                    options.ChangesHandler?.Invoke(existingDoublet, existingDoublet);
                    return link.Index;
                }
            }
        }

        /// <summary>
        /// Helper to conditionally write to console if tracing is enabled.
        /// </summary>
        private static void TraceIfEnabled(Options options, string message)
        {
            if (options.Trace)
            {
                Console.WriteLine(message);
            }
        }
    }
}