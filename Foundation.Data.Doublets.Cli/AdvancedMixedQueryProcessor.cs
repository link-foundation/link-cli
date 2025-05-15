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

            /// <summary>
            /// Enables extra console tracing of internal steps if true.
            /// </summary>
            public bool Trace { get; set; } = false;

            public NamedLinks<uint>? NamedLinks { get; set; }

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

            // We expect something like (( restriction ) ( substitution ))
            var outerLink = parsedLinks[0];
            var outerLinkValues = outerLink.Values;
            if (outerLinkValues == null || outerLinkValues.Count < 2)
            {
                TraceIfEnabled(options, "[ProcessQuery] Outer link has fewer than 2 sub-links, returning.");
                return;
            }

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

            // Build pattern lists from the sub-links
            var restrictionPatterns = restrictionLink.Values ?? new List<LinoLink>();
            var substitutionPatterns = substitutionLink.Values ?? new List<LinoLink>();

            TraceIfEnabled(options, $"[ProcessQuery] Restriction patterns to parse: {restrictionPatterns.Count}");
            TraceIfEnabled(options, $"[ProcessQuery] Substitution patterns to parse: {substitutionPatterns.Count}");

            var restrictionInternalPatterns = restrictionPatterns
                .Select(l => CreatePatternFromLino(l))
                .ToList();

            var substitutionInternalPatterns = substitutionPatterns
                .Select(l => CreatePatternFromLino(l))
                .ToList();

            // ----------------------------------------------------------------
            // FIX: If we see restrictionLink with exactly 1 sub-link => that sub-link has 2 sub-values => no IDs => interpret as a single composite pattern
            if (
                string.IsNullOrEmpty(restrictionLink.Id) &&
                restrictionLink.Values?.Count == 1
            )
            {
                var single = restrictionLink.Values[0];
                if (
                    string.IsNullOrEmpty(single.Id) &&
                    single.Values?.Count == 2
                )
                {
                    // Create a single composite pattern from ((1 *) (* 2))
                    var topLevelPattern = CreatePatternFromLino(single);

                    // If it doesn't have an explicit index or if it's "*", force a variable ID, so we don't unify with #1/#2
                    if (string.IsNullOrEmpty(topLevelPattern.Index) || topLevelPattern.Index == "*")
                    {
                        topLevelPattern.Index = "$top_" + Guid.NewGuid().ToString("N");
                        TraceIfEnabled(options, $"[ProcessQuery] Assigned a variable index => {topLevelPattern.Index}");
                    }

                    // Clear out the multiple sub-pattern expansions and replace with our single composite pattern
                    restrictionInternalPatterns.Clear();
                    restrictionInternalPatterns.Add(topLevelPattern);

                    TraceIfEnabled(options,
                        "[ProcessQuery] Detected single sub-link (no ID) with 2 sub-values => replaced with one composite restriction pattern.");
                }
            }
            // ----------------------------------------------------------------

            // If restrictionLink.Id is not empty => treat it as an extra pattern
            if (!string.IsNullOrEmpty(restrictionLink.Id))
            {
                TraceIfEnabled(options, "[ProcessQuery] Restriction link has non-empty Id => adding extra pattern for it.");
                var extraRestrictionPattern = CreatePatternFromLino(restrictionLink);
                restrictionInternalPatterns.Insert(0, extraRestrictionPattern);
            }

            // If substitutionLink.Id is not empty => treat it as an extra pattern
            if (!string.IsNullOrEmpty(substitutionLink.Id))
            {
                TraceIfEnabled(options, "[ProcessQuery] Substitution link has non-empty Id => adding extra pattern for it.");
                var extraSubstitutionPattern = CreatePatternFromLino(substitutionLink);
                substitutionInternalPatterns.Insert(0, extraSubstitutionPattern);
            }

            TraceIfEnabled(options, "[ProcessQuery] Converting restriction patterns => done.");
            TraceIfEnabled(options, "[ProcessQuery] Converting substitution patterns => done.");

            TraceIfEnabled(options, "[ProcessQuery] Finding solutions for restriction patterns...");
            var solutions = FindAllSolutions(links, restrictionInternalPatterns);

            TraceIfEnabled(options, $"[ProcessQuery] Found {solutions.Count} total solution(s) matching restriction patterns.");
            if (solutions.Count == 0)
            {
                TraceIfEnabled(options, "[ProcessQuery] No solutions found => returning.");
                return;
            }

            // Decide if all solutions would lead to a no-op
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

                    TraceIfEnabled(options,
                        "[ProcessQuery] For a solution => " +
                        $"substitution links count={substitutionLinks.Count}, restriction links count={restrictionLinks.Count}.");

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
                            TraceIfEnabled(options, $"[ProcessQuery] Detected unexpected deletion of link #{beforeLink.Index} => will restore later.");
                        }
                    }
                    return originalHandler?.Invoke(before, after) ?? links.Constants.Continue;
                };

                TraceIfEnabled(options, "[ProcessQuery] Applying all planned operations...");
                ApplyAllPlannedOperations(links, allPlannedOperations, options);

                TraceIfEnabled(options, "[ProcessQuery] Restoring unexpected deletions if any...");
                RestoreUnexpectedLinkDeletions(links, unexpectedDeletions, intendedFinalStates, options);
            }

            TraceIfEnabled(options, "[ProcessQuery] Finished processing query.");
        }

        /// <summary>
        /// Recursively ensures that a LinoLink (potentially nested) is created. 
        /// Returns the numeric ID or ANY if leaf/unparseable.
        /// </summary>
        private static uint EnsureNestedLinkCreatedRecursively(ILinks<uint> links, LinoLink lino, Options options)
        {
            var nullConstant = links.Constants.Null;
            var anyConstant = links.Constants.Any;
            var namedLinks = options.NamedLinks;

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
                    // Set name if NamedLinks is present and lino.Id is not numeric and not '*'
                    if (namedLinks != null && !IsNumericOrStar(lino.Id))
                    {
                        namedLinks.SetName(parsedNumber, lino.Id);
                    }
                    return parsedNumber;
                }
                // If not numeric and not '*', treat as a named entity: create a new link and set its name
                if (namedLinks != null)
                {
                    var existingId = namedLinks.GetByName(lino.Id);
                    if (existingId != links.Constants.Null)
                    {
                        TraceIfEnabled(options, $"[EnsureNestedLinkCreatedRecursively] Found existing named leaf '{lino.Id}' => ID={existingId}");
                        return existingId;
                    }
                    // Try to use UnicodeStringStorage's GetOrCreateType if available
                    if (options is { NamedLinks: NamedLinks<uint> named } && named.GetType().Name == "NamedLinks`1")
                    {
                        // Try to find a UnicodeStringStorage in the test context
                        // This is a hack: in real code, pass a delegate or factory
                        var unicodeStringStorageType = typeof(Foundation.Data.Doublets.Cli.UnicodeStringStorage<uint>);
                        var storageField = options.GetType().GetField("UnicodeStringStorage", System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.Public | System.Reflection.BindingFlags.NonPublic);
                        if (storageField != null)
                        {
                            var storage = storageField.GetValue(options) as dynamic;
                            if (storage != null)
                            {
                                uint newId = storage.GetOrCreateType(lino.Id);
                                TraceIfEnabled(options, $"[EnsureNestedLinkCreatedRecursively] Created new named leaf '{lino.Id}' via UnicodeStringStorage => ID={newId}");
                                return newId;
                            }
                        }
                    }
                    // fallback: try to create a new link and set name
                    uint fallbackId = 0;
                    links.CreateAndUpdate(links.Constants.Null, links.Constants.Null, (before, after) =>
                    {
                        var afterLink = new DoubletLink(after);
                        if (fallbackId == 0 && afterLink.Index != 0 && afterLink.Index != links.Constants.Any)
                        {
                            fallbackId = afterLink.Index;
                        }
                        return links.Constants.Continue;
                    });
                    if (fallbackId == 0 || fallbackId == links.Constants.Any)
                    {
                        fallbackId = links.SearchOrDefault(links.Constants.Null, links.Constants.Null);
                    }
                    namedLinks.SetName(fallbackId, lino.Id);
                    TraceIfEnabled(options, $"[EnsureNestedLinkCreatedRecursively] Created new named leaf '{lino.Id}' => ID={fallbackId}");
                    return fallbackId;
                }
                TraceIfEnabled(options, $"[EnsureNestedLinkCreatedRecursively] Leaf with unparseable and no NamedLinks => returning ANY.");
                return anyConstant;
            }

            // If 2 Values => interpret as a composite link
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
                    $"[EnsureNestedLinkCreatedRecursively] Created/ensured composite => (index={index}, src={sourceId}, trg={targetId}) => actual ID={createdId}");
                // Set name if NamedLinks is present and lino.Id is not numeric and not '*'
                if (namedLinks != null && !string.IsNullOrEmpty(lino.Id) && !IsNumericOrStar(lino.Id))
                {
                    namedLinks.SetName(createdId, lino.Id);
                }
                return createdId;
            }

            // If more than 2 => do nothing special => ANY
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
                TraceIfEnabled(options, $"[RestoreUnexpectedLinkDeletions] We have {unexpectedDeletions.Count} unexpected deletion(s).");
                foreach (var deletedLink in unexpectedDeletions)
                {
                    if (finalIntendedStates.TryGetValue(deletedLink.Index, out var intendedLink))
                    {
                        if (intendedLink.Index == 0)
                        {
                            TraceIfEnabled(options, $"[RestoreUnexpectedLinkDeletions] Link #{deletedLink.Index} was intended-deletion => skip restore.");
                            continue;
                        }
                        if (!links.Exists(intendedLink.Index))
                        {
                            TraceIfEnabled(options, $"[RestoreUnexpectedLinkDeletions] Recreating link #{deletedLink.Index} => was unexpected deletion.");
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

        private static List<(DoubletLink before, DoubletLink after)> DetermineOperationsFromPatterns(
            List<DoubletLink> restrictions,
            List<DoubletLink> substitutions,
            ILinks<uint> links)
        {
            var anyOrZero = new HashSet<uint> { 0, links.Constants.Any };

            var normalRestrictions = restrictions.Where(r => !anyOrZero.Contains(r.Index)).ToList();
            var wildcardRestrictions = restrictions.Where(r => anyOrZero.Contains(r.Index)).ToList();

            var normalSubstitutions = substitutions.Where(s => !anyOrZero.Contains(s.Index)).ToList();
            var wildcardSubstitutions = substitutions.Where(s => anyOrZero.Contains(s.Index)).ToList();

            var restrictionByIndex = normalRestrictions.ToDictionary(r => r.Index, r => r);
            var substitutionByIndex = normalSubstitutions.ToDictionary(s => s.Index, s => s);

            var operations = new List<(DoubletLink before, DoubletLink after)>();
            var allIndices = restrictionByIndex.Keys.Union(substitutionByIndex.Keys).ToList();

            // Step 1) For each distinct index in normal restrictions & substitutions
            foreach (var idx in allIndices)
            {
                bool hasRestriction = restrictionByIndex.TryGetValue(idx, out var rLink);
                bool hasSubstitution = substitutionByIndex.TryGetValue(idx, out var sLink);

                if (hasRestriction && hasSubstitution)
                {
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

            // Step 2) Wildcard restrictions => each is a separate "delete"
            foreach (var rLink in wildcardRestrictions)
            {
                operations.Add((rLink, default(DoubletLink)));
            }

            // Step 3) Wildcard substitutions => each is a separate "create"
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
                    TraceIfEnabled(options, $"[ApplyAllPlannedOperations] Deleting link => ID={before.Index}, S={before.Source}, T={before.Target}");
                    RemoveLinks(links, before, options);
                }
                else if (before.Index == 0 && after.Index != 0)
                {
                    TraceIfEnabled(options, $"[ApplyAllPlannedOperations] Creating link => ID={after.Index}, S={after.Source}, T={after.Target}");
                    CreateOrUpdateLink(links, after, options);
                }
                else if (before.Index != 0 && after.Index != 0)
                {
                    if (before.Source != after.Source || before.Target != after.Target)
                    {
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
                        TraceIfEnabled(options, $"[ApplyAllPlannedOperations] No changes for link => ID={before.Index} => no-op.");
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
            ILinks<uint> links,
            Pattern pattern,
            Dictionary<string, uint> currentSolution)
        {
            if (pattern.IsLeaf)
            {
                uint idx = ResolveId(links, pattern.Index, currentSolution);
                uint any = links.Constants.Any;
                var candidates = links.All(new DoubletLink(idx, any, any));
                foreach (var link in candidates)
                {
                    var candidateLink = new DoubletLink(link);
                    var assignments = new Dictionary<string, uint>();
                    AssignVariableIfNeeded(pattern.Index, candidateLink.Index, assignments);
                    yield return assignments;
                }
                yield break;
            }

            var anyVal = links.Constants.Any;
            bool idxIsVar = IsVariable(pattern.Index);
            bool idxIsAny = pattern.Index == "*";
            uint idxResolved = ResolveId(links, pattern.Index, currentSolution);

            // If idxResolved is a known link => skip enumerating everything
            if (!idxIsVar && !idxIsAny && idxResolved != anyVal && idxResolved != 0 && links.Exists(idxResolved))
            {
                var link = new DoubletLink(links.GetLink(idxResolved));
                var sourceMatches = RecursiveMatchSubPattern(links, pattern.Source, link.Source, currentSolution);
                foreach (var sourceSol in sourceMatches)
                {
                    var targetMatches = RecursiveMatchSubPattern(links, pattern.Target, link.Target, sourceSol);
                    foreach (var targetSol in targetMatches)
                    {
                        var combined = new Dictionary<string, uint>(targetSol);
                        AssignVariableIfNeeded(pattern.Index, idxResolved, combined);
                        yield return combined;
                    }
                }
            }
            else
            {
                // Otherwise we iterate over all links
                var allLinks = links.All(new DoubletLink(anyVal, anyVal, anyVal));
                foreach (var raw in allLinks)
                {
                    var candidateLink = new DoubletLink(raw);
                    if (!CheckIdMatch(links, pattern.Index, candidateLink.Index, currentSolution)) 
                        continue;

                    var sourceMatches = RecursiveMatchSubPattern(links, pattern.Source, candidateLink.Source, currentSolution);
                    foreach (var sourceSol in sourceMatches)
                    {
                        var targetMatches = RecursiveMatchSubPattern(links, pattern.Target, candidateLink.Target, sourceSol);
                        foreach (var targetSol in targetMatches)
                        {
                            var combined = new Dictionary<string, uint>(targetSol);
                            AssignVariableIfNeeded(pattern.Index, candidateLink.Index, combined);
                            yield return combined;
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
                    var newSol = new Dictionary<string, uint>(currentSolution);
                    AssignVariableIfNeeded(pattern.Index, linkId, newSol);
                    yield return newSol;
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
            foreach (var sourceSol in sourceMatches)
            {
                var targetMatches = RecursiveMatchSubPattern(links, pattern.Target, link.Target, sourceSol);
                foreach (var targetSol in targetMatches)
                {
                    var combined = new Dictionary<string, uint>(targetSol);
                    AssignVariableIfNeeded(pattern.Index, link.Index, combined);
                    yield return combined;
                }
            }
        }

        private static bool CheckIdMatch(
            ILinks<uint> links, 
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

        private static uint ResolveId(
            ILinks<uint> links,
            string identifier,
            Dictionary<string, uint> currentSolution)
        {
            var anyVal = links.Constants.Any;
            if (string.IsNullOrEmpty(identifier)) return anyVal;
            if (currentSolution.TryGetValue(identifier, out var value))
            {
                return value;
            }
            if (IsVariable(identifier))
            {
                return anyVal;
            }
            if (TryParseLinkId(identifier, links.Constants, ref anyVal))
            {
                return anyVal;
            }
            return anyVal;
        }

        private static bool DetermineIfSolutionIsNoOperation(
            Dictionary<string, uint> solution,
            List<Pattern> restrictions,
            List<Pattern> substitutions,
            ILinks<uint> links)
        {
            var substitutedRestrictions = restrictions
                .Select(r => ApplySolutionToPattern(links, solution, r))
                .Where(link => link != null)
                .Select(link => new DoubletLink(link!))
                .ToList();

            var substitutedSubstitutions = substitutions
                .Select(s => ApplySolutionToPattern(links, solution, s))
                .Where(link => link != null)
                .Select(link => new DoubletLink(link!))
                .ToList();

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
            ILinks<uint> links,
            Dictionary<string, uint> solution,
            Pattern? pattern)
        {
            if (pattern == null) return null;

            if (pattern.IsLeaf)
            {
                uint index = ResolveId(links, pattern.Index, solution);
                var anyVal = links.Constants.Any;
                return new DoubletLink(index, anyVal, anyVal);
            }
            else
            {
                uint idx = ResolveId(links, pattern.Index, solution);
                var sourceLink = ApplySolutionToPattern(links, solution, pattern.Source);
                var targetLink = ApplySolutionToPattern(links, solution, pattern.Target);

                uint anyVal = links.Constants.Any;
                uint finalSource = sourceLink?.Index ?? anyVal;
                uint finalTarget = targetLink?.Index ?? anyVal;

                if (finalSource == 0) finalSource = anyVal;
                if (finalTarget == 0) finalTarget = anyVal;

                return new DoubletLink(idx, finalSource, finalTarget);
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
                    TraceIfEnabled(options, $"[CreateOrUpdateLink] Link #{link.Index} doesn't exist => ensuring creation.");
                    LinksExtensions.EnsureCreated(links, link.Index);
                }
                var existingLink = links.GetLink(link.Index);
                var existingDoublet = new DoubletLink(existingLink);

                if (existingDoublet.Source != link.Source || existingDoublet.Target != link.Target)
                {
                    TraceIfEnabled(options,
                        $"[CreateOrUpdateLink] Updating link #{link.Index}: {existingDoublet.Source}->{link.Source}, {existingDoublet.Target}->{link.Target}.");
                    LinksExtensions.EnsureCreated(links, link.Index);
                    options.ChangesHandler?.Invoke(
                        new DoubletLink(link.Index, nullConstant, nullConstant),
                        new DoubletLink(link.Index, nullConstant, nullConstant)
                    );
                    links.Update(
                        new DoubletLink(link.Index, anyConstant, anyConstant),
                        link,
                        (b, a) => options.ChangesHandler?.Invoke(b, a) ?? links.Constants.Continue
                    );
                }
                else
                {
                    TraceIfEnabled(options, $"[CreateOrUpdateLink] Link #{link.Index} is already S={link.Source}, T={link.Target} => no change.");
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
                    TraceIfEnabled(options,
                        $"[CreateOrUpdateLink] Creating new link => (S={link.Source},T={link.Target}).");
                    links.CreateAndUpdate(link.Source, link.Target, (before, after) =>
                    {
                        var afterLink = new DoubletLink(after);
                        if (newCreatedId == 0 && afterLink.Index != 0 && afterLink.Index != anyConstant)
                        {
                            newCreatedId = afterLink.Index;
                            TraceIfEnabled(options, $"[CreateOrUpdateLink] => assigned new ID={newCreatedId}");
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
                    // Already exists
                    TraceIfEnabled(options, $"[CreateOrUpdateLink] Link already found => ID={found}, no changes.");
                    var existingLink = new DoubletLink(found, link.Source, link.Target);
                    options.ChangesHandler?.Invoke(existingLink, existingLink);
                }
            }
        }

        private static void RemoveLinks(ILinks<uint> links, DoubletLink restriction, Options options)
        {
            var linksToRemove = links.All(restriction)
                                     .Where(l => l != null)
                                     .Select(l => new DoubletLink(l))
                                     .ToList();

            TraceIfEnabled(options,
                $"[RemoveLinks] Found {linksToRemove.Count} link(s) matching (ID={restriction.Index}, S={restriction.Source}, T={restriction.Target}).");

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
            if (string.IsNullOrEmpty(id)) return false;
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
            else if (uint.TryParse(id, out uint linkVal))
            {
                parsedValue = linkVal;
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
                var sPat = CreatePatternFromLino(lino.Values[0]);
                var tPat = CreatePatternFromLino(lino.Values[1]);
                return new Pattern(lino.Id ?? "", sPat, tPat);
            }

            // If more than 2 => treat similarly to leaf with ID
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
                    TraceIfEnabled(options, $"[EnsureLinkCreated] Creating link for (S={link.Source}, T={link.Target}).");
                    links.CreateAndUpdate(link.Source, link.Target, (before, after) =>
                    {
                        var afterLink = new DoubletLink(after);
                        if (createdIndex == 0 && afterLink.Index != 0 && afterLink.Index != anyConstant)
                        {
                            createdIndex = afterLink.Index;
                            TraceIfEnabled(options, $"[EnsureLinkCreated] => assigned new ID={createdIndex}");
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
                    TraceIfEnabled(options, $"[EnsureLinkCreated] Link already found => ID={existingIndex} => no-op.");
                    var existing = new DoubletLink(existingIndex, link.Source, link.Target);
                    options.ChangesHandler?.Invoke(existing, existing);
                    return existingIndex;
                }
            }
            else
            {
                // We have an index => ensure created or updated
                if (!links.Exists(link.Index))
                {
                    TraceIfEnabled(options, $"[EnsureLinkCreated] Link #{link.Index} doesn't exist => ensuring creation.");
                    LinksExtensions.EnsureCreated(links, link.Index);
                }
                var stored = links.GetLink(link.Index);
                var storedD = new DoubletLink(stored);
                if (storedD.Source != link.Source || storedD.Target != link.Target)
                {
                    TraceIfEnabled(options,
                        $"[EnsureLinkCreated] Updating link #{link.Index} => {storedD.Source}->{link.Source}, {storedD.Target}->{link.Target}.");
                    uint finalIndex = link.Index;
                    links.Update(new DoubletLink(link.Index, anyConstant, anyConstant), link, (b, a) =>
                        options.ChangesHandler?.Invoke(b, a) ?? links.Constants.Continue);
                    return finalIndex;
                }
                else
                {
                    TraceIfEnabled(options, $"[EnsureLinkCreated] Link #{link.Index} is already correct => no-op.");
                    options.ChangesHandler?.Invoke(storedD, storedD);
                    return link.Index;
                }
            }
        }

        // Helper for link naming logic
        private static bool IsNumericOrStar(string? id)
        {
            if (string.IsNullOrEmpty(id)) return false;
            if (id == "*") return true;
            uint dummy;
            return uint.TryParse(id, out dummy);
        }

        private static void TraceIfEnabled(Options options, string message)
        {
            if (options.Trace)
            {
                Console.WriteLine(message);
            }
        }
    }
}