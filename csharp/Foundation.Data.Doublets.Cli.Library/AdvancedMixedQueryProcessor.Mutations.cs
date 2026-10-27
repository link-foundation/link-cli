using System.Numerics;
// Part of the partial AdvancedMixedQueryProcessor class.
// Applying a solution to the store: creating, updating and removing doublets.
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
        private static void CreateOrUpdateLink(INamedTypesLinks<T> links, LinkParts<T> linkDefinition, Options options)
        {
            var nullConstant = links.Constants.Null;
            var anyConstant = links.Constants.Any;

            // Wildcard substitution rename: delegate to nested creation with proper naming
            if (linkDefinition.Index == anyConstant)
            {
                TraceIfEnabled(options, "[CreateOrUpdateLink] Detected wildcard substitution => nested create & name.");
                var parsed = QueryParser.Parse<T>(options.Query ?? string.Empty);
                if (parsed.Count > 0)
                {
                    var outer = parsed[0];
                    if (outer.Values != null && outer.Values.Count > 1)
                    {
                        var substitutionLinoLink = outer.Values[1];
                        if (substitutionLinoLink.Values != null)
                        {
                            foreach (var composite in substitutionLinoLink.Values)
                            {
                                EnsureNestedLinkCreatedRecursively(links, composite, options);
                            }
                        }
                    }
                }
                return;
            }

            if (linkDefinition.Index != nullConstant)
            {
                // update existing link
                EnsureAddress(links, linkDefinition.Index, options);
                var existingLinkRecord = links.GetLink(linkDefinition.Index);
                var existingDoublet = new LinkParts<T>(existingLinkRecord);

                if (existingDoublet.Source != linkDefinition.Source || existingDoublet.Target != linkDefinition.Target)
                {
                    TraceIfEnabled(options,
                        $"[CreateOrUpdateLink] Updating link #{linkDefinition.Index}: {existingDoublet.Source}->{linkDefinition.Source}, {existingDoublet.Target}->{linkDefinition.Target}.");
                    links.Update(
                        new LinkParts<T>(linkDefinition.Index, anyConstant, anyConstant),
                        linkDefinition,
                        (beforeState, afterState) =>
                            options.ChangesHandler?.Invoke(beforeState, afterState) ?? links.Constants.Continue
                    );
                }
                else
                {
                    TraceIfEnabled(options, $"[CreateOrUpdateLink] Link #{linkDefinition.Index} is already S={linkDefinition.Source}, T={linkDefinition.Target} => no change.");
                    options.ChangesHandler?.Invoke(existingDoublet, existingDoublet);
                }
            }
            else
            {
                // create new link
                var existingLinkIndex = links.SearchOrDefault(linkDefinition.Source, linkDefinition.Target);
                if (existingLinkIndex == default)
                {
                    T newLinkIndex = T.Zero;
                    TraceIfEnabled(options,
                        $"[CreateOrUpdateLink] Creating new link => (S={linkDefinition.Source},T={linkDefinition.Target}).");
                    links.CreateAndUpdate(linkDefinition.Source, linkDefinition.Target, (beforeState, afterState) =>
                    {
                        var afterLinkRecord = new LinkParts<T>(afterState);
                        if (newLinkIndex == T.Zero && afterLinkRecord.Index != T.Zero && afterLinkRecord.Index != anyConstant)
                        {
                            newLinkIndex = afterLinkRecord.Index;
                            TraceIfEnabled(options, $"[CreateOrUpdateLink] => assigned new ID={newLinkIndex}");
                        }
                        return options.ChangesHandler?.Invoke(beforeState, afterState) ?? links.Constants.Continue;
                    });

                    if (newLinkIndex == T.Zero || newLinkIndex == anyConstant)
                    {
                        newLinkIndex = links.SearchOrDefault(linkDefinition.Source, linkDefinition.Target);
                    }
                }
                else
                {
                    TraceIfEnabled(options, $"[CreateOrUpdateLink] Link already found => ID={existingLinkIndex}, no changes.");
                    var existingLink = new LinkParts<T>(existingLinkIndex, linkDefinition.Source, linkDefinition.Target);
                    options.ChangesHandler?.Invoke(existingLink, existingLink);
                }
            }
        }

        private static void RemoveLinks(
            INamedTypesLinks<T> links,
            LinkParts<T> restriction,
            Options options)
        {
            var linksToRemove = links.All(restriction)
                                     .Where(l => l != null)
                                     .Select(l => new LinkParts<T>(l))
                                     .ToList();

            TraceIfEnabled(options,
                $"[RemoveLinks] Found {linksToRemove.Count} link(s) matching (ID={restriction.Index}, S={restriction.Source}, T={restriction.Target}).");

            foreach (var link in linksToRemove)
            {
                if (links.Exists(link.Index))
                {
                    // Remove the name before deleting
                    links.RemoveName(link.Index);
                    TraceIfEnabled(options, $"[RemoveLinks] Deleting link => ID={link.Index}, S={link.Source}, T={link.Target}");
                    links.Delete(link, (before, after) =>
                        options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue);
                }
            }
        }

        private static LinkParts<T> ConvertToDoubletLink(INamedTypesLinks<T> links, LinoLink linoLink, T defaultValue)
        {
            T index = defaultValue;
            T source = defaultValue;
            T target = defaultValue;
            TryParseLinkId(linoLink.Id, links, ref index);
            if (linoLink.Values?.Count == 2)
            {
                var sourceLink = linoLink.Values[0];
                TryParseLinkId(sourceLink.Id, links, ref source);
                var targetLink = linoLink.Values[1];
                TryParseLinkId(targetLink.Id, links, ref target);
            }
            return new LinkParts<T>(index, source, target);
        }

        private static bool TryParseLinkId(string? id, INamedTypesLinks<T> links, ref T parsedValue)
        {
            if (string.IsNullOrEmpty(id)) return false;
            if (id == "*")
            {
                parsedValue = links.Constants.Any;
                return true;
            }
            else if (id.EndsWith(":"))
            {
                var trimmed = id.TrimEnd(':');
                if (TryParseAddress(trimmed, out T linkId))
                {
                    parsedValue = linkId;
                    return true;
                }
                // Try to resolve as string alias
                var aliasId = links.GetByName(QueryParser.Name(trimmed));
                if (aliasId != links.Constants.Null)
                {
                    parsedValue = aliasId;
                    return true;
                }
            }
            else if (TryParseAddress(id, out T linkVal))
            {
                parsedValue = linkVal;
                return true;
            }
            else
            {
                // Try to resolve as string alias
                var aliasId = links.GetByName(QueryParser.Name(id));
                if (aliasId != links.Constants.Null)
                {
                    parsedValue = aliasId;
                    return true;
                }
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

        private static Pattern CreatePatternFromLino(LinoLink linkNode)
        {
            if (linkNode.Values == null || linkNode.Values.Count == 0)
            {
                return new Pattern(linkNode.Id ?? "");
            }

            if (linkNode.Values.Count == 2)
            {
                var sourcePattern = CreatePatternFromLino(linkNode.Values[0]);
                var targetPattern = CreatePatternFromLino(linkNode.Values[1]);
                return new Pattern(linkNode.Id ?? "", sourcePattern, targetPattern);
            }

            // If more than 2 => treat similarly to leaf with ID
            return new Pattern(linkNode.Id ?? "");
        }

        private static T EnsureLinkCreated(INamedTypesLinks<T> links, LinkParts<T> link, Options options)
        {
            var nullConstant = links.Constants.Null;
            var anyConstant = links.Constants.Any;

            if (link.Index == nullConstant)
            {
                // If no index => search or create
                var existingIndex = links.SearchOrDefault(link.Source, link.Target);
                if (existingIndex == default)
                {
                    T createdIndex = T.Zero;
                    TraceIfEnabled(options, $"[EnsureLinkCreated] Creating link for (S={link.Source}, T={link.Target}).");
                    links.CreateAndUpdate(link.Source, link.Target, (before, after) =>
                    {
                        var afterLink = new LinkParts<T>(after);
                        if (createdIndex == T.Zero && afterLink.Index != T.Zero && afterLink.Index != anyConstant)
                        {
                            createdIndex = afterLink.Index;
                            TraceIfEnabled(options, $"[EnsureLinkCreated] => assigned new ID={createdIndex}");
                        }
                        return options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;
                    });

                    if (createdIndex == T.Zero || createdIndex == anyConstant)
                    {
                        createdIndex = links.SearchOrDefault(link.Source, link.Target);
                    }
                    return createdIndex;
                }
                else
                {
                    TraceIfEnabled(options, $"[EnsureLinkCreated] Link already found => ID={existingIndex} => no-op.");
                    var existing = new LinkParts<T>(existingIndex, link.Source, link.Target);
                    options.ChangesHandler?.Invoke(existing, existing);
                    return existingIndex;
                }
            }
            else
            {
                // We have an index => ensure created or updated
                EnsureAddress(links, link.Index, options);
                var stored = links.GetLink(link.Index);
                var storedD = new LinkParts<T>(stored);
                if (storedD.Source != link.Source || storedD.Target != link.Target)
                {
                    TraceIfEnabled(options,
                        $"[EnsureLinkCreated] Updating link #{link.Index} => {storedD.Source}->{link.Source}, {storedD.Target}->{link.Target}.");
                    T finalIndex = link.Index;
                    links.Update(new LinkParts<T>(link.Index, anyConstant, anyConstant), link, (beforeState, afterState) =>
                        options.ChangesHandler?.Invoke(beforeState, afterState) ?? links.Constants.Continue);
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
            T dummy;
            return TryParseAddress(id, out dummy);
        }

        private static void TraceIfEnabled(Options options, string message)
        {
            if (options.Trace)
            {
                DiagnosticOutput.WriteLine(message);
            }
        }

        /// <summary>
        /// Fills <paramref name="index"/> in when the store lacks it and reports the empty
        /// <c>(index: 0 0)</c> link that creates.
        /// </summary>
        private static void EnsureAddress(INamedTypesLinks<T> links, T index, Options options)
        {
            if (links.Exists(index))
            {
                return;
            }
            TraceIfEnabled(options, $"[EnsureAddress] Link #{index} doesn't exist => ensuring creation.");
            LinksExtensions.EnsureCreated(links, index);
            ReportCreatedEmpty(links, index, options);
        }

        private static void ReportCreatedEmpty(INamedTypesLinks<T> links, T index, Options options)
        {
            var nothing = links.Constants.Null;
            options.ChangesHandler?.Invoke(new LinkParts<T>(nothing, nothing, nothing), new LinkParts<T>(index, nothing, nothing));
        }

        /// <summary>
        /// The link called <paramref name="name"/>, created and reported as the point link
        /// <c>(name: name name)</c> when there is none yet.
        /// </summary>
        private static T EnsureNamedPointLink(INamedTypesLinks<T> links, string name, Options options)
        {
            var existing = links.GetByName(QueryParser.Name(name));
            if (existing != links.Constants.Null)
            {
                return existing;
            }
            var created = links.Create();
            ReportCreatedEmpty(links, created, options);
            links.SetName(created, QueryParser.Name(name));
            ApplyCompositeUpdate(links, created, created, created, options);
            TraceIfEnabled(options, $"[EnsureNamedPointLink] Created point link '{name}' => ID={created}");
            return created;
        }

        /// <summary>
        /// Points the existing link <paramref name="id"/> at <paramref name="source"/> and
        /// <paramref name="target"/>; a link that already does is reported unchanged and not rewritten.
        /// </summary>
        private static void ApplyCompositeUpdate(INamedTypesLinks<T> links, T id, T source, T target, Options options)
        {
            var stored = new LinkParts<T>(links.GetLink(id));
            if (stored.Source == source && stored.Target == target)
            {
                options.ChangesHandler?.Invoke(stored, stored);
                return;
            }
            TraceIfEnabled(options, $"[EnsureNestedLinkCreatedRecursively] Updating link ID={id} => Source={source}, Target={target}");
            links.Update(new LinkParts<T>(id, links.Constants.Any, links.Constants.Any), new LinkParts<T>(id, source, target), ReportChanges(links, options));
        }

        private static WriteHandler<T> ReportChanges(INamedTypesLinks<T> links, Options options) =>
            (before, after) => options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;

        /// <summary>
        /// Detects a two-child composite pattern where at least one child matches the composite identifier.
        /// </summary>
        private static bool TryGetTwoChildCompositePattern(
            LinoLink pattern,
            out string compositeIdentifier,
            out LinoLink leftPattern,
            out LinoLink rightPattern)
        {
            compositeIdentifier = pattern.Id ?? string.Empty;
            leftPattern = default!;
            rightPattern = default!;
            if (!string.IsNullOrEmpty(compositeIdentifier)
                && pattern.Values != null
                && pattern.Values.Count == 2)
            {
                leftPattern = pattern.Values[0];
                rightPattern = pattern.Values[1];
                // Only detect composites when one or both children share the identifier
                if (leftPattern.Id == compositeIdentifier || rightPattern.Id == compositeIdentifier)
                {
                    return true;
                }
            }
            return false;
        }

        private enum CompositeCase { Self, LeftMix, RightMix }

        private static CompositeCase ClassifyCompositeCase(string name, LinoLink left, LinoLink right)
        {
            if (left.Id == name && right.Id == name) return CompositeCase.Self;
            if (left.Id == name && right.Id != name) return CompositeCase.LeftMix;
            if (left.Id != name && right.Id == name) return CompositeCase.RightMix;
            throw new InvalidOperationException($"Invalid composite pattern for name '{name}'");
        }

        private static T HandleStringComposite(string name, LinoLink left, LinoLink right, INamedTypesLinks<T> links, Options options)
        {
            var id = EnsureNamedPointLink(links, name, options);
            var caseType = ClassifyCompositeCase(name, left, right);
            switch (caseType)
            {
                case CompositeCase.Self:
                    ApplyCompositeUpdate(links, id, id, id, options);
                    return id;
                case CompositeCase.LeftMix:
                    {
                        var otherId = EnsureNestedLinkCreatedRecursively(links, right, options);
                        ApplyCompositeUpdate(links, id, id, otherId, options);
                        return id;
                    }
                case CompositeCase.RightMix:
                    {
                        var otherId = EnsureNestedLinkCreatedRecursively(links, left, options);
                        ApplyCompositeUpdate(links, id, otherId, id, options);
                        return id;
                    }
                default:
                    throw new InvalidOperationException($"Unhandled composite case {caseType}");
            }
        }

        /// <summary>
        /// Resolves a single leaf pattern into its numeric or named link ID.
        /// </summary>
        private static T ResolveLeaf(LinoLink pattern, INamedTypesLinks<T> links, Options options)
        {
            var nullConstant = links.Constants.Null;
            var anyConstant = links.Constants.Any;

            if (string.IsNullOrEmpty(pattern.Id))
            {
                TraceIfEnabled(options, "[EnsureNestedLinkCreatedRecursively] Leaf with empty ID => returning ANY.");
                return anyConstant;
            }
            if (pattern.Id == "*")
            {
                TraceIfEnabled(options, "[EnsureNestedLinkCreatedRecursively] Leaf with '*' => returning ANY.");
                return anyConstant;
            }
            if (pattern.Id.StartsWith("$"))
            {
                TraceIfEnabled(options, "[EnsureNestedLinkCreatedRecursively] Variable leaf => returning ANY.");
                return anyConstant;
            }
            if (TryParseAddress(pattern.Id, out T parsedNumber))
            {
                TraceIfEnabled(options, $"[EnsureNestedLinkCreatedRecursively] Leaf parse => returning {parsedNumber}.");
                return parsedNumber;
            }
            return EnsureNamedPointLink(links, pattern.Id, options);
        }

        /// <summary>
        /// Ensures a composite link exists with the given index or named identifier and child IDs.
        /// </summary>
        private static T CreateCompositeLink(
            string? literalIdentifier,
            T sourceLinkId,
            T targetLinkId,
            INamedTypesLinks<T> links,
            Options options)
        {
            // Determine the numeric index for the composite: default 0, wildcard, or parsed from identifier
            T compositeIndex = T.Zero;
            var wildcardIndex = links.Constants.Any;
            if (!string.IsNullOrEmpty(literalIdentifier))
            {
                if (literalIdentifier == "*")
                {
                    compositeIndex = wildcardIndex;
                }
                else
                {
                    var identifierClean = literalIdentifier.Replace(":", string.Empty);
                    if (TryParseAddress(identifierClean, out var parsedIndex))
                    {
                        compositeIndex = parsedIndex;
                    }
                }
            }
            var isName = !string.IsNullOrEmpty(literalIdentifier) && !IsNumericOrStar(literalIdentifier) && !literalIdentifier.StartsWith("$");
            if (isName)
            {
                // A name is an address too: the link it already names is updated in place.
                var named = links.GetByName(QueryParser.Name(literalIdentifier!));
                if (named != links.Constants.Null)
                {
                    compositeIndex = named;
                }
            }
            // Build the composite link structure and ensure it exists
            var compositeLinkDefinition = new LinkParts<T>(compositeIndex, sourceLinkId, targetLinkId);
            var compositeLinkId = EnsureLinkCreated(links, compositeLinkDefinition, options);
            TraceIfEnabled(options, $"[EnsureNestedLinkCreatedRecursively] Created or ensured composite link => Index={compositeIndex}, Source={sourceLinkId}, Target={targetLinkId} => Actual ID={compositeLinkId}");
            if (isName)
            {
                links.SetName(compositeLinkId, QueryParser.Name(literalIdentifier!));
            }
            return compositeLinkId;
        }
    }
}
