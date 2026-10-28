using System.Numerics;
// Part of the partial AdvancedMixedQueryProcessor class.
// Reference validation and auto-creation of links a substitution refers to.
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
        private static void ValidateLinksExistOrWillBeCreated(
            INamedTypesLinks<T> links,
            IList<LinoLink> restrictionPatterns,
            IList<LinoLink> substitutionPatterns,
            Options options)
        {
            if (options.SkipReferenceValidation) return;
            TraceIfEnabled(options, "[ValidateLinksExistOrWillBeCreated] Starting validation");

            while (true)
            {
                var finalIds = links.ReserveQueryAllocations(preview => ProcessQuery(preview, new Options
                {
                    Query = options.Query,
                    SkipReferenceValidation = true,
                }));
                var plan = PlanReferences(links, restrictionPatterns, substitutionPatterns, options, finalIds);
                if (plan.MissingReferences.Count == 0)
                {
                    TraceIfEnabled(options, "[ValidateLinksExistOrWillBeCreated] Validation completed");
                    return;
                }

                if (!options.AutoCreateMissingReferences)
                {
                    var missing = plan.MissingReferences[0];
                    throw new InvalidOperationException(
                      $"Invalid reference to non-existent link '{QueryParser.Name(missing.Identifier)}' in {missing.PatternType} pattern. " +
                      $"Link '{QueryParser.Name(missing.Identifier)}' does not exist and will not be created by this operation. " +
                      "Use --auto-create-missing-references to create missing references as point links."
                    );
                }

                // Creating a reference frees addresses on the way, which changes the ones the query's
                // new links get, so the plan is made again until every reference is accounted for.
                AutoCreateMissingReferences(links, plan, options);
            }
        }

        private static LinkReferencePlan PlanReferences(
            INamedTypesLinks<T> links,
            IList<LinoLink> restrictionPatterns,
            IList<LinoLink> substitutionPatterns,
            Options options,
            IReadOnlySet<T>? finalIds)
        {
            var plan = BuildLinkReferencePlan(links, substitutionPatterns, finalIds);

            TraceIfEnabled(options, $"[ValidateLinksExistOrWillBeCreated] Numeric links to be created: {string.Join(", ", plan.NumericIdsToBeCreated.OrderBy(id => id))}");
            TraceIfEnabled(options, $"[ValidateLinksExistOrWillBeCreated] Named links to be created: {string.Join(", ", plan.NamesToBeCreated.OrderBy(name => name, StringComparer.Ordinal))}");

            CollectMissingReferences(restrictionPatterns, links, plan, false, "restriction", options);
            CollectMissingReferences(substitutionPatterns, links, plan, true, "substitution", options);
            return plan;
        }

        private sealed class LinkReferencePlan
        {
            public HashSet<T> NumericIdsToBeCreated { get; } = new();
            public HashSet<string> NamesToBeCreated { get; } = new(StringComparer.Ordinal);
            public HashSet<(T Source, T Target)> CompositePairsToBeCreated { get; } = new();
            public List<MissingLinkReference> MissingReferences { get; } = new();
            private readonly HashSet<string> _missingReferenceKeys = new(StringComparer.Ordinal);

            public void AddMissingReference(MissingLinkReference reference)
            {
                if (_missingReferenceKeys.Add(reference.Key))
                {
                    MissingReferences.Add(reference);
                }
            }
        }

        private sealed class MissingLinkReference
        {
            public required string Identifier { get; init; }
            public required string PatternType { get; init; }
            public required T? NumericId { get; init; }
            public string Key => NumericId.HasValue ? $"id:{NumericId.Value}" : $"name:{Identifier}";
        }

        private static LinkReferencePlan BuildLinkReferencePlan(INamedTypesLinks<T> links, IList<LinoLink> substitutionPatterns, IReadOnlySet<T>? finalIds)
        {
            var plan = new LinkReferencePlan();
            var anonymousLinks = 0;
            foreach (var pattern in substitutionPatterns)
            {
                CollectDefinitions(pattern, plan, ref anonymousLinks);
            }
            plan.NumericIdsToBeCreated.UnionWith(finalIds ?? (IEnumerable<T>)NextCreatedAddresses(links, anonymousLinks));
            return plan;
        }

        /// <summary>
        /// Collects the ids and names the substitution defines, the (source, target) pairs it writes
        /// under them, and how many links it writes without an id.
        /// </summary>
        private static void CollectDefinitions(LinoLink pattern, LinkReferencePlan plan, ref int anonymousLinks)
        {
            if (pattern.Values != null)
            {
                foreach (var subPattern in pattern.Values)
                {
                    CollectDefinitions(subPattern, plan, ref anonymousLinks);
                }
            }
            if (!IsComposite(pattern))
            {
                return;
            }
            if (!TryGetConcreteIdentifier(pattern.Id, out var identifier))
            {
                anonymousLinks++;
                return;
            }
            if (TryParseAddress(identifier, out var linkId))
            {
                plan.NumericIdsToBeCreated.Add(linkId);
            }
            else
            {
                plan.NamesToBeCreated.Add(identifier);
            }
            if (TryGetConcreteNumericIdentifier(pattern.Values![0].Id, out var source) &&
                TryGetConcreteNumericIdentifier(pattern.Values[1].Id, out var target))
            {
                plan.CompositePairsToBeCreated.Add((source, target));
            }
        }

        /// <summary>The addresses the next <paramref name="count"/> links created in <paramref name="links"/> get.</summary>
        /// <remarks>
        /// Compatibility fallback for stores without query reservations. A recycling store may
        /// hand out its last freed address first. Creating and deleting in reverse preserves that
        /// order; monotonic stores must provide ReserveQueryAllocations instead.
        /// </remarks>
        private static List<T> NextCreatedAddresses(INamedTypesLinks<T> links, int count)
        {
            var addresses = Enumerable.Range(0, count).Select(_ => links.Create()).ToList();
            for (var i = addresses.Count - 1; i >= 0; i--)
            {
                links.Delete(addresses[i]);
            }
            return addresses;
        }

        private static void CollectMissingReferences(
            IList<LinoLink> patterns,
            INamedTypesLinks<T> links,
            LinkReferencePlan plan,
            bool isSubstitution,
            string patternType,
            Options options)
        {
            foreach (var pattern in patterns)
            {
                CollectMissingReferences(pattern, links, plan, isSubstitution, patternType, options);
            }
        }

        private static void CollectMissingReferences(
            LinoLink pattern,
            INamedTypesLinks<T> links,
            LinkReferencePlan plan,
            bool isSubstitution,
            string patternType,
            Options options)
        {
            var patternIdIsDefinition = isSubstitution && IsComposite(pattern) && TryGetConcreteIdentifier(pattern.Id, out var _ignoredIdentifier);

            if (!patternIdIsDefinition && TryGetConcreteIdentifier(pattern.Id, out var identifier))
            {
                ValidateReferenceIdentifier(identifier, links, plan, patternType, options);
            }

            if (pattern.Values != null)
            {
                foreach (var subPattern in pattern.Values)
                {
                    CollectMissingReferences(subPattern, links, plan, isSubstitution, patternType, options);
                }
            }
        }

        private static void ValidateReferenceIdentifier(
            string identifier,
            INamedTypesLinks<T> links,
            LinkReferencePlan plan,
            string patternType,
            Options options)
        {
            if (TryParseAddress(identifier, out var linkId))
            {
                if (!links.Exists(linkId) && !plan.NumericIdsToBeCreated.Contains(linkId))
                {
                    plan.AddMissingReference(new MissingLinkReference
                    {
                        Identifier = identifier,
                        PatternType = patternType,
                        NumericId = linkId
                    });
                    return;
                }
                TraceIfEnabled(options, $"[ValidateReferencesInPattern] Link {linkId} reference validated in {patternType} pattern");
                return;
            }

            if (links.GetByName(QueryParser.Name(identifier)) == links.Constants.Null && !plan.NamesToBeCreated.Contains(identifier))
            {
                plan.AddMissingReference(new MissingLinkReference
                {
                    Identifier = identifier,
                    PatternType = patternType,
                    NumericId = null
                });
                return;
            }

            TraceIfEnabled(options, $"[ValidateReferencesInPattern] Named link '{identifier}' reference validated in {patternType} pattern");
        }

        private static void AutoCreateMissingReferences(
            INamedTypesLinks<T> links,
            LinkReferencePlan plan,
            Options options)
        {
            foreach (var missing in plan.MissingReferences.Where(reference => reference.NumericId.HasValue).OrderBy(reference => reference.NumericId!.Value))
            {
                var linkId = missing.NumericId!.Value;
                if (links.Exists(linkId))
                {
                    continue;
                }

                TraceIfEnabled(options, $"[ValidateLinksExistOrWillBeCreated] Auto-creating missing numeric reference {linkId}.");
                EnsureAddress(links, linkId, options);
                if (plan.CompositePairsToBeCreated.Contains((linkId, linkId)))
                {
                    TraceIfEnabled(options, $"[ValidateLinksExistOrWillBeCreated] Link {linkId} exists as a placeholder because ({linkId}, {linkId}) is defined by the substitution.");
                    continue;
                }
                links.Update(
                  new LinkParts<T>(linkId, links.Constants.Null, links.Constants.Null),
                  new LinkParts<T>(linkId, linkId, linkId),
                  (beforeState, afterState) =>
                      options.ChangesHandler?.Invoke(beforeState, afterState) ?? links.Constants.Continue
                );
            }

            foreach (var missing in plan.MissingReferences.Where(reference => !reference.NumericId.HasValue).OrderBy(reference => reference.Identifier, StringComparer.Ordinal))
            {
                if (links.GetByName(QueryParser.Name(missing.Identifier)) != links.Constants.Null)
                {
                    continue;
                }

                TraceIfEnabled(options, $"[ValidateLinksExistOrWillBeCreated] Auto-creating missing named reference '{QueryParser.Name(missing.Identifier)}' as point link.");
                EnsureNamedPointLink(links, missing.Identifier, options);
            }
        }

        private static bool IsComposite(LinoLink pattern) => pattern.Values?.Count == 2;

        private static bool TryGetConcreteIdentifier(string? id, out string identifier)
        {
            identifier = string.Empty;
            if (string.IsNullOrWhiteSpace(id))
            {
                return false;
            }

            identifier = id.TrimEnd(':');
            if (identifier.Length == 0 || identifier == "*" || identifier.StartsWith("$"))
            {
                return false;
            }

            return true;
        }

        private static bool TryGetConcreteNumericIdentifier(string? id, out T linkId)
        {
            linkId = T.Zero;
            return TryGetConcreteIdentifier(id, out var identifier) && TryParseAddress(identifier, out linkId);
        }
    }
}
