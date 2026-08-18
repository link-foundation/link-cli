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
        public class Options
        {
            public string? Query { get; set; }
            public WriteHandler<uint>? ChangesHandler { get; set; }

            /// <summary>
            /// Enables extra console tracing of internal steps if true.
            /// </summary>
            public bool Trace { get; set; } = false;

            /// <summary>
            /// Creates missing numeric and named references as self-referential point links instead of failing validation.
            /// </summary>
            public bool AutoCreateMissingReferences { get; set; } = false;

            public static implicit operator Options(string query) => new Options { Query = query };
        }

        public static void ProcessQuery(INamedTypesLinks<uint> links, Options options)
        {
            ArgumentNullException.ThrowIfNull(links);
            ArgumentNullException.ThrowIfNull(options);

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

                // VALIDATION: Validate that all references in creation scenario are valid
                try
                {
                    var emptyRestrictionPatterns = new List<LinoLink>();
                    ValidateLinksExistOrWillBeCreated(links, emptyRestrictionPatterns, substitutionLink.Values ?? new List<LinoLink>(), options);
                }
                catch (Exception ex)
                {
                    TraceIfEnabled(options, $"[ProcessQuery] Creation validation failed: {ex.Message}");
                    throw;
                }

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

            // VALIDATION: Check that all referenced links exist or will be created
            try
            {
                ValidateLinksExistOrWillBeCreated(links, restrictionPatterns, substitutionPatterns, options);
            }
            catch (Exception ex)
            {
                TraceIfEnabled(options, $"[ProcessQuery] Validation failed: {ex.Message}");
                throw;
            }

            var restrictionInternalPatterns = restrictionPatterns
                .Select(l => CreatePatternFromLino(l))
                .ToList();

            var substitutionInternalPatterns = substitutionPatterns
                .Select(l => CreatePatternFromLino(l))
                .ToList();

            // ----------------------------------------------------------------
            // FIX: If we see restrictionLink with exactly 1 sub-link => that sub-link has 2 sub-values => interpret as a single composite pattern
            // This handles patterns like ((() (1 2))) where the outer restriction has a single composite child
            if (
                string.IsNullOrEmpty(restrictionLink.Id) &&
                restrictionLink.Values?.Count == 1
            )
            {
                var single = restrictionLink.Values[0];
                // Check if this is a composite (has 2 sub-values) and doesn't have a numeric/wildcard ID
                if (
                    single.Values?.Count == 2 &&
                    (string.IsNullOrEmpty(single.Id) || !IsNumericOrStar(single.Id))
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
                        "[ProcessQuery] Detected single sub-link with 2 sub-values => replaced with one composite restriction pattern.");
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
                    var substitutionLinks = ApplySolutionToPatterns(links, solution, substitutionInternalPatterns, isSubstitution: true);
                    var restrictionLinks = ApplySolutionToPatterns(links, solution, restrictionInternalPatterns, isSubstitution: false);

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

                try
                {
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
                }
                finally
                {
                    options.ChangesHandler = originalHandler;
                }

                TraceIfEnabled(options, "[ProcessQuery] Restoring unexpected deletions if any...");
                RestoreUnexpectedLinkDeletions(links, unexpectedDeletions, intendedFinalStates, options);
            }

            TraceIfEnabled(options, "[ProcessQuery] Finished processing query.");
        }

        /// <summary>
        /// Recursively ensures that a LinoLink (potentially nested) is created. 
        /// Returns the numeric ID or ANY if leaf/unparseable.
        /// </summary>
        private static uint EnsureNestedLinkCreatedRecursively(INamedTypesLinks<uint> links, LinoLink pattern, Options options)
        {
            var nullConstant = links.Constants.Null;
            var anyConstant = links.Constants.Any;

            // Handle string-based two-child composites
            if (TryGetTwoChildCompositePattern(pattern, out var name, out var left, out var right) && !IsNumericOrStar(name))
            {
                return HandleStringComposite(name, left, right, links, options);
            }

            if (pattern.Values == null || pattern.Values.Count == 0)
            {
                return ResolveLeaf(pattern, links, options);
            }

            // If 2 Values => interpret as a composite link
            if (pattern.Values.Count == 2)
            {
                var sourceId = EnsureNestedLinkCreatedRecursively(links, pattern.Values[0], options);
                var targetId = EnsureNestedLinkCreatedRecursively(links, pattern.Values[1], options);

                // Generic composite creation for numeric or non-matching patterns
                return CreateCompositeLink(pattern.Id, sourceId, targetId, links, options);
            }

            // If more than 2 => do nothing special => ANY
            TraceIfEnabled(options, "[EnsureNestedLinkCreatedRecursively] More than 2 sub-values => returning ANY.");
            return anyConstant;
        }

        private static void RestoreUnexpectedLinkDeletions(
            INamedTypesLinks<uint> links,
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
            INamedTypesLinks<uint> links)
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
            foreach (var linkIndex in allIndices)
            {
                bool hasRestriction = restrictionByIndex.TryGetValue(linkIndex, out var restrictionLink);
                bool hasSubstitution = substitutionByIndex.TryGetValue(linkIndex, out var substitutionLink);

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
                    // Deletion
                    operations.Add((restrictionLink, default(DoubletLink)));
                }
                else if (!hasRestriction && hasSubstitution)
                {
                    // Creation
                    operations.Add((default(DoubletLink), substitutionLink));
                }
            }

            // Step 2) Wildcard restrictions => each is a separate "delete"
            foreach (var restrictionLink in wildcardRestrictions)
            {
                operations.Add((restrictionLink, default(DoubletLink)));
            }

            // Step 3) Wildcard substitutions => each is a separate "create"
            foreach (var substitutionLink in wildcardSubstitutions)
            {
                operations.Add((default(DoubletLink), substitutionLink));
            }

            return operations;
        }

        private static void ApplyAllPlannedOperations(
            INamedTypesLinks<uint> links,
            List<(DoubletLink before, DoubletLink after)> operations,
            Options options)
        {
            foreach (var (before, after) in operations)
            {
                TraceIfEnabled(options, $"[ApplyAllPlannedOperations] Operation: before=({before.Index}:{before.Source}->{before.Target}), after=({after.Index}:{after.Source}->{after.Target})");
                if (before.Index != 0)
                {
                    var beforeName = links.GetName(before.Index);
                    TraceIfEnabled(options, $"[ApplyAllPlannedOperations] Name for before.Index {before.Index} = '{beforeName}'");
                }
                if (after.Index != 0)
                {
                    var afterNamePre = links.GetName(after.Index);
                    TraceIfEnabled(options, $"[ApplyAllPlannedOperations] Name for after.Index {after.Index} = '{afterNamePre}' (pre-op)");
                }
                if (before.Index != 0 && after.Index == 0)
                {
                    TraceIfEnabled(options, $"[ApplyAllPlannedOperations] Deleting link => ID={before.Index}, S={before.Source}, T={before.Target}");
                    RemoveLinks(links, before, options);
                }
                else if (before.Index == 0 && (after.Index != 0 || after.Source != 0 || after.Target != 0))
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
                            links.Update(before, after, (beforeState, afterState) =>
                                options.ChangesHandler?.Invoke(beforeState, afterState) ?? links.Constants.Continue);
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
                if (after.Index != 0)
                {
                    var afterNamePost = links.GetName(after.Index);
                    TraceIfEnabled(options, $"[ApplyAllPlannedOperations] Name for after.Index {after.Index} = '{afterNamePost}' (post-op)");
                }
            }
        }
    }
}
