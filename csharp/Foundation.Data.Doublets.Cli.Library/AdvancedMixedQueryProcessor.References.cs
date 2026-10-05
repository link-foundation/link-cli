// Part of the partial AdvancedMixedQueryProcessor class.
// Reference validation and auto-creation of links a substitution refers to.
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
        private static void ValidateLinksExistOrWillBeCreated(
            INamedTypesLinks<uint> links,
            IList<LinoLink> restrictionPatterns,
            IList<LinoLink> substitutionPatterns,
            Options options)
        {
            TraceIfEnabled(options, "[ValidateLinksExistOrWillBeCreated] Starting validation");

            while (true)
            {
                var plan = PlanReferences(links, restrictionPatterns, substitutionPatterns, options);
                if (plan.MissingReferences.Count == 0)
                {
                    TraceIfEnabled(options, "[ValidateLinksExistOrWillBeCreated] Validation completed");
                    return;
                }

                if (!options.AutoCreateMissingReferences)
                {
                    var missing = plan.MissingReferences[0];
                    throw new InvalidOperationException(
                      $"Invalid reference to non-existent link '{missing.Identifier}' in {missing.PatternType} pattern. " +
                      $"Link '{missing.Identifier}' does not exist and will not be created by this operation. " +
                      "Use --auto-create-missing-references to create missing references as point links."
                    );
                }

                // Creating a reference frees addresses on the way, which changes the ones the query's
                // new links get, so the plan is made again until every reference is accounted for.
                AutoCreateMissingReferences(links, plan, options);
            }
        }

        private static LinkReferencePlan PlanReferences(
            INamedTypesLinks<uint> links,
            IList<LinoLink> restrictionPatterns,
            IList<LinoLink> substitutionPatterns,
            Options options)
        {
            var plan = BuildLinkReferencePlan(links, substitutionPatterns);

            TraceIfEnabled(options, $"[ValidateLinksExistOrWillBeCreated] Numeric links to be created: {string.Join(", ", plan.NumericIdsToBeCreated.OrderBy(id => id))}");
            TraceIfEnabled(options, $"[ValidateLinksExistOrWillBeCreated] Named links to be created: {string.Join(", ", plan.NamesToBeCreated.OrderBy(name => name, StringComparer.Ordinal))}");

            CollectMissingReferences(restrictionPatterns, links, plan, false, "restriction", options);
            CollectMissingReferences(substitutionPatterns, links, plan, true, "substitution", options);
            return plan;
        }

        private sealed class LinkReferencePlan
        {
            public HashSet<uint> NumericIdsToBeCreated { get; } = new();
            public HashSet<string> NamesToBeCreated { get; } = new(StringComparer.Ordinal);
            public HashSet<(uint Source, uint Target)> CompositePairsToBeCreated { get; } = new();
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
            public required uint? NumericId { get; init; }
            public string Key => NumericId.HasValue ? $"id:{NumericId.Value}" : $"name:{Identifier}";
        }

        private static LinkReferencePlan BuildLinkReferencePlan(INamedTypesLinks<uint> links, IList<LinoLink> substitutionPatterns)
        {
            var plan = new LinkReferencePlan();
            var anonymousLinks = 0;
            foreach (var pattern in substitutionPatterns)
            {
                CollectDefinitions(pattern, plan, ref anonymousLinks);
            }
            plan.NumericIdsToBeCreated.UnionWith(NextCreatedAddresses(links, anonymousLinks));
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
            if (uint.TryParse(identifier, out var linkId))
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
        /// A store reuses the address freed last first, so the lowest free one is not necessarily next.
        /// Creating the links and deleting them again in reverse leaves the store exactly as it was,
        /// and is the one way to ask any store, a remote one included.
        /// </remarks>
        private static List<uint> NextCreatedAddresses(INamedTypesLinks<uint> links, int count)
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
            INamedTypesLinks<uint> links,
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
            INamedTypesLinks<uint> links,
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
            INamedTypesLinks<uint> links,
            LinkReferencePlan plan,
            string patternType,
            Options options)
        {
            if (uint.TryParse(identifier, out var linkId))
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

            if (links.GetByName(identifier) == links.Constants.Null && !plan.NamesToBeCreated.Contains(identifier))
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
            INamedTypesLinks<uint> links,
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
                  new DoubletLink(linkId, links.Constants.Null, links.Constants.Null),
                  new DoubletLink(linkId, linkId, linkId),
                  (beforeState, afterState) =>
                      options.ChangesHandler?.Invoke(beforeState, afterState) ?? links.Constants.Continue
                );
            }

            foreach (var missing in plan.MissingReferences.Where(reference => !reference.NumericId.HasValue).OrderBy(reference => reference.Identifier, StringComparer.Ordinal))
            {
                if (links.GetByName(missing.Identifier) != links.Constants.Null)
                {
                    continue;
                }

                TraceIfEnabled(options, $"[ValidateLinksExistOrWillBeCreated] Auto-creating missing named reference '{missing.Identifier}' as point link.");
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

        private static bool TryGetConcreteNumericIdentifier(string? id, out uint linkId)
        {
            linkId = 0;
            return TryGetConcreteIdentifier(id, out var identifier) && uint.TryParse(identifier, out linkId);
        }
    }
}
