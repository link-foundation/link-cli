using Platform.Data.Doublets;
using Platform.Protocols.Lino;
using LinoLink = Platform.Protocols.Lino.Link<string>;
using DoubletLink = Platform.Data.Doublets.Link<uint>;
using System.Numerics;
using Platform.Data;
using Platform.Delegates;
using System.Collections.Generic;
using System.Linq;
using Platform.Converters;
using System;

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
            Console.WriteLine("=== START QUERY PROCESSING ===");
            Console.WriteLine("Query: " + options.Query);

            PrintAllLinks("Initial State", links);

            var query = options.Query;
            var @null = links.Constants.Null;
            var any = links.Constants.Any;
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

            // Print parsed outer link
            Console.WriteLine("Parsed Restriction Link: " + LinoToString(restrictionLink));
            Console.WriteLine("Parsed Substitution Link: " + LinoToString(substitutionLink));

            // If both sides are empty, no operation
            if ((restrictionLink.Values?.Count == 0) && (substitutionLink.Values?.Count == 0))
            {
                return;
            }

            // Handle simple create scenario
            if (restrictionLink.Values?.Count == 0 && (substitutionLink.Values?.Count ?? 0) > 0)
            {
                Console.WriteLine("Simple Create Scenario");
                foreach (var linkToCreate in substitutionLink.Values ?? new List<LinoLink>())
                {
                    var doubletLink = ToDoubletLink(links, linkToCreate, @null);
                    Console.WriteLine("Create link: " + doubletLink);
                    Set(links, doubletLink, options);
                }
                PrintAllLinks("Final State after create", links);
                return;
            }

            // Handle simple delete scenario
            if (substitutionLink.Values?.Count == 0 && (restrictionLink.Values?.Count ?? 0) > 0)
            {
                Console.WriteLine("Simple Delete Scenario");
                foreach (var linkToDelete in restrictionLink.Values ?? new List<LinoLink>())
                {
                    var queryLink = ToDoubletLink(links, linkToDelete, any);
                    Console.WriteLine("Delete link matching: " + queryLink);
                    Unset(links, queryLink, options);
                }
                PrintAllLinks("Final State after delete", links);
                return;
            }

            // More complex scenario: both have values, possibly with variables.
            var restrictionPatterns = restrictionLink.Values ?? new List<LinoLink>();
            var substitutionPatterns = substitutionLink.Values ?? new List<LinoLink>();

            var restrictionPatternsInternal = restrictionPatterns.Select(l => PatternFromLino(links, l)).ToList();
            var substitutionPatternsInternal = substitutionPatterns.Select(l => PatternFromLino(links, l)).ToList();

            if (!string.IsNullOrEmpty(restrictionLink.Id))
            {
                restrictionPatternsInternal.Insert(0, PatternFromLino(links, restrictionLink));
            }
            if (!string.IsNullOrEmpty(substitutionLink.Id))
            {
                substitutionPatternsInternal.Insert(0, PatternFromLino(links, substitutionLink));
            }

            Console.WriteLine("Restriction Patterns:");
            foreach (var p in restrictionPatternsInternal)
            {
                Console.WriteLine($"  ({p.Index}: {p.Source} {p.Target})");
            }

            Console.WriteLine("Substitution Patterns:");
            foreach (var p in substitutionPatternsInternal)
            {
                Console.WriteLine($"  ({p.Index}: {p.Source} {p.Target})");
            }

            var solutions = FindAllSolutions(links, restrictionPatternsInternal);

            Console.WriteLine("Found Solutions:");
            if (solutions.Count == 0)
            {
                Console.WriteLine("  No solutions found.");
                return;
            }
            else
            {
                int solNum = 1;
                foreach (var sol in solutions)
                {
                    Console.WriteLine("  Solution #" + solNum++);
                    foreach (var kv in sol)
                    {
                        Console.WriteLine($"    {kv.Key} = {kv.Value}");
                    }
                }
            }

            bool allNoOp = true;
            foreach (var sol in solutions)
            {
                if (!CheckIfNoOp(sol, restrictionPatternsInternal, substitutionPatternsInternal))
                {
                    allNoOp = false;
                    break;
                }
            }
            Console.WriteLine("No-Op check: " + (allNoOp ? "All solutions produce no changes" : "At least one solution changes something"));

            // Two-phase approach
            var allOperations = new List<(DoubletLink before, DoubletLink after)>();

            if (allNoOp)
            {
                // Just read all matches from all solutions
                Console.WriteLine("No-Op scenario, just reading matched links");
                foreach (var solution in solutions)
                {
                    var matchedLinks = ExtractMatchedLinksFromSolution(links, solution, restrictionPatternsInternal);
                    foreach (var link in matchedLinks)
                    {
                        // No changes, just read
                        allOperations.Add((link, link));
                    }
                }
            }
            else
            {
                // Collect sets/updates/unsets from all solutions
                Console.WriteLine("Changes scenario, collecting operations from each solution");
                foreach (var solution in solutions)
                {
                    var substitutionDoublets = substitutionPatternsInternal
                        .Select(pattern => ApplySolutionToPattern(links, solution, pattern))
                        .ToList();
                    var restrictionDoublets = restrictionPatternsInternal
                        .Select(pattern => ApplySolutionToPattern(links, solution, pattern))
                        .ToList();

                    var solutionOps = ComputeOperationsFromPatterns(restrictionDoublets, substitutionDoublets);
                    allOperations.AddRange(solutionOps);
                }
            }

            Console.WriteLine("All Operations to Apply:");
            foreach (var (before, after) in allOperations)
            {
                Console.WriteLine($"  Before: {FormatLink(before)}  After: {FormatLink(after)}");
            }

            // Apply all operations after collecting them
            if (allNoOp)
            {
                // Just read all
                foreach (var (before, after) in allOperations)
                {
                    options.ChangesHandler?.Invoke(before, after);
                }
            }
            else
            {
                ApplyAllOperations(links, allOperations, options);
            }

            PrintAllLinks("Final State after query", links);
            Console.WriteLine("=== END QUERY PROCESSING ===");
        }

        #region Utilities for Debugging

        private static void PrintAllLinks(string title, ILinks<uint> links)
        {
            Console.WriteLine($"--- {title} ---");
            var any = links.Constants.Any;
            var all = links.All(new DoubletLink(any, any, any));
            foreach (var l in all)
            {
                var dl = new DoubletLink(l);
                Console.WriteLine(FormatLink(dl));
            }
        }

        private static string FormatLink(DoubletLink link)
        {
            if (link.Index == 0 && link.Source == 0 && link.Target == 0)
            {
                return "(no link)";
            }
            return $"({link.Index}: {link.Source} {link.Target})";
        }

        private static string LinoToString(LinoLink lino)
        {
            var id = lino.Id ?? "";
            var vals = lino.Values?.Select(v => v.Id ?? "").ToList() ?? new List<string>();
            return $"({id}: {string.Join(" ", vals)})";
        }

        #endregion

        #region Two-Phase Operation Helpers

        private static List<(DoubletLink before, DoubletLink after)> ComputeOperationsFromPatterns(List<DoubletLink> restrictions, List<DoubletLink> substitutions)
        {
            var rByIndex = restrictions.Where(r => r.Index != 0).ToDictionary(d => d.Index, d => d);
            var sByIndex = substitutions.Where(s => s.Index != 0).ToDictionary(d => d.Index, d => d);

            var allIndices = rByIndex.Keys.Union(sByIndex.Keys).ToList();

            var operations = new List<(DoubletLink before, DoubletLink after)>();

            // Debug:
            Console.WriteLine("Computing Operations from Patterns:");
            Console.WriteLine("Restrictions:");
            foreach (var r in restrictions) Console.WriteLine("  " + FormatLink(r));
            Console.WriteLine("Substitutions:");
            foreach (var s in substitutions) Console.WriteLine("  " + FormatLink(s));

            foreach (var idx in allIndices)
            {
                var hasR = rByIndex.TryGetValue(idx, out var rlink);
                var hasS = sByIndex.TryGetValue(idx, out var slink);

                if (hasR && hasS)
                {
                    // Update if different
                    if (rlink.Source != slink.Source || rlink.Target != slink.Target)
                    {
                        operations.Add((rlink, slink));
                    }
                    else
                    {
                        // No actual change, treat as read
                        operations.Add((rlink, rlink));
                    }
                }
                else if (hasR && !hasS)
                {
                    // Delete
                    operations.Add((rlink, default(DoubletLink)));
                }
                else if (!hasR && hasS)
                {
                    // Create
                    operations.Add((default(DoubletLink), slink));
                }
            }

            return operations;
        }

        private static void ApplyAllOperations(ILinks<uint> links, List<(DoubletLink before, DoubletLink after)> operations, Options options)
        {
            Console.WriteLine("Applying All Operations:");
            foreach (var (before, after) in operations)
            {
                Console.WriteLine($"Operation: Before = {FormatLink(before)}, After = {FormatLink(after)}");
                if (before.Index != 0 && after.Index == 0)
                {
                    // Delete
                    Unset(links, before, options);
                }
                else if (before.Index == 0 && after.Index != 0)
                {
                    // Create
                    Set(links, after, options);
                }
                else if (before.Index != 0 && after.Index != 0)
                {
                    // Update or read
                    if (before.Source != after.Source || before.Target != after.Target)
                    {
                        links.Update(before, after, (b, a) =>
                        {
                            Console.WriteLine($"Update callback: Before={FormatLink(new DoubletLink(b))}, After={FormatLink(new DoubletLink(a))}");
                            return options.ChangesHandler?.Invoke(b, a) ?? links.Constants.Continue;
                        });
                    }
                    else
                    {
                        // Read only
                        options.ChangesHandler?.Invoke(before, before);
                    }
                }
                else
                {
                    // No operation
                }
            }
        }

        #endregion

        #region Matching and Solutions

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
                        if (Unify(solution, match))
                        {
                            var combined = new Dictionary<string, uint>(solution);
                            foreach (var kv in match)
                            {
                                combined[kv.Key] = kv.Value;
                            }
                            newSolutions.Add(combined);
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

        private static bool Unify(Dictionary<string, uint> currentSolution, Dictionary<string, uint> newAssignments)
        {
            foreach (var kv in newAssignments)
            {
                if (currentSolution.TryGetValue(kv.Key, out var existingVal))
                {
                    if (existingVal != kv.Value)
                    {
                        return false; // conflict
                    }
                }
            }
            return true;
        }

        private static IEnumerable<Dictionary<string, uint>> MatchPattern(ILinks<uint> links, Pattern pattern, Dictionary<string, uint> currentSolution)
        {
            uint indexVal = ResolveId(links, pattern.Index, currentSolution);
            uint sourceVal = ResolveId(links, pattern.Source, currentSolution);
            uint targetVal = ResolveId(links, pattern.Target, currentSolution);

            var candidates = links.All(new DoubletLink(indexVal, sourceVal, targetVal));
            // Debug
            Console.WriteLine($"MatchPattern: Pattern=({pattern.Index}: {pattern.Source} {pattern.Target}), Resolved=({indexVal}: {sourceVal} {targetVal}), Candidates={candidates.Count()}");

            foreach (var link in candidates)
            {
                var candidateLink = new DoubletLink(link);
                var assignments = new Dictionary<string, uint>();
                if (IsVariable(pattern.Index) || pattern.Index == "*:")
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

        private static bool IsVariable(string id)
        {
            return !string.IsNullOrEmpty(id) && id.StartsWith("$");
        }

        private static uint ResolveId(ILinks<uint> links, string id, Dictionary<string, uint> currentSolution)
        {
            if (string.IsNullOrEmpty(id)) return links.Constants.Any;

            if (currentSolution.TryGetValue(id, out var val))
            {
                return val;
            }

            if (id == "*") return links.Constants.Any;
            if (id == "*:") return links.Constants.Any;
            if (uint.TryParse(id, out var parsed))
            {
                return parsed;
            }

            // If it's a variable but not assigned yet, treat as ANY
            if (IsVariable(id))
            {
                return links.Constants.Any;
            }

            return links.Constants.Any;
        }

        private static bool CheckIfNoOp(Dictionary<string, uint> solution, List<Pattern> restrictions, List<Pattern> substitutions)
        {
            var substitutedRestrictions = restrictions.Select(r => ApplySolutionToPattern(null, solution, r)).ToList();
            var substitutedSubstitutions = substitutions.Select(s => ApplySolutionToPattern(null, solution, s)).ToList();

            substitutedRestrictions.Sort((a,b) => a.Index.CompareTo(b.Index));
            substitutedSubstitutions.Sort((a,b) => a.Index.CompareTo(b.Index));

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

        private static List<DoubletLink> ExtractMatchedLinksFromSolution(ILinks<uint> links, Dictionary<string, uint> solution, List<Pattern> patterns)
        {
            var result = new List<DoubletLink>();
            foreach (var pattern in patterns)
            {
                var dbl = ApplySolutionToPattern(links, solution, pattern);
                var candidates = links.All(dbl);
                foreach (var c in candidates)
                {
                    result.Add(new DoubletLink(c));
                }
            }
            return result.Distinct().ToList();
        }

        private static DoubletLink ApplySolutionToPattern(ILinks<uint> links, Dictionary<string, uint> solution, Pattern pattern)
        {
            uint ApplyVar(string id)
            {
                uint anyVal = links?.Constants.Any ?? 0;
                if (string.IsNullOrEmpty(id)) return anyVal;
                if (solution.TryGetValue(id, out var val)) return val;
                if (id == "*" || id == "*:") return anyVal;
                if (uint.TryParse(id, out var p)) return p;
                return anyVal;
            }

            uint i = ApplyVar(pattern.Index);
            uint s = ApplyVar(pattern.Source);
            uint t = ApplyVar(pattern.Target);
            return new DoubletLink(i, s, t);
        }

        #endregion

        #region Set/Unset Methods

        static void Set(ILinks<uint> links, DoubletLink substitutionLink, Options options)
        {
            var @null = links.Constants.Null;
            var any = links.Constants.Any;
            Console.WriteLine("Set operation: " + FormatLink(substitutionLink));
            if (substitutionLink.Source == any)
            {
                throw new ArgumentException($"The source of the link {substitutionLink} cannot be any.");
            }
            if (substitutionLink.Target == any)
            {
                throw new ArgumentException($"The target of the link {substitutionLink} cannot be any.");
            }
            if (substitutionLink.Index != @null)
            {
                EnsureCreated(links, substitutionLink.Index);
                var restrictionDoublet = new DoubletLink(substitutionLink.Index, any, any);
                options.ChangesHandler?.Invoke(null, restrictionDoublet);
                links.Update(restrictionDoublet, substitutionLink, (before, after) =>
                {
                    Console.WriteLine("Update during Set: Before=" + FormatLink(new DoubletLink(before)) + ", After=" + FormatLink(new DoubletLink(after)));
                    return options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;
                });
            }
            else
            {
                var linkIndex = links.SearchOrDefault(substitutionLink.Source, substitutionLink.Target);
                if (linkIndex == default)
                {
                    linkIndex = links.CreateAndUpdate(substitutionLink.Source, substitutionLink.Target, (before, after) =>
                    {
                        Console.WriteLine("CreateAndUpdate during Set: Before=" + FormatLink(new DoubletLink(before)) + ", After=" + FormatLink(new DoubletLink(after)));
                        return options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;
                    });
                }
                else
                {
                    var existingLink = new DoubletLink(linkIndex, substitutionLink.Source, substitutionLink.Target);
                    Console.WriteLine("Existing link found during Set: " + FormatLink(existingLink));
                    options.ChangesHandler?.Invoke(existingLink, existingLink);
                }
            }
        }

        static void Unset(ILinks<uint> links, DoubletLink restrictionLink, Options options)
        {
            Console.WriteLine("Unset operation for: " + FormatLink(restrictionLink));
            var linksToDelete = links.All(restrictionLink);
            foreach (var link in linksToDelete)
            {
                Console.WriteLine("Deleting link: " + FormatLink(new DoubletLink(link)));
                links.Delete(link, (before, after) =>
                {
                    Console.WriteLine("Delete callback: Before=" + FormatLink(new DoubletLink(before)) + ", After=" + FormatLink(new DoubletLink(after)));
                    return options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue;
                });
            }
        }

        static DoubletLink ToDoubletLink(ILinks<uint> links, LinoLink linoLink, uint defaultValue)
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

        static void TryParseLinkId(string? id, LinksConstants<uint> constants, ref uint parsedValue)
        {
            if (string.IsNullOrEmpty(id))
            {
                return;
            }
            if (id == "*")
            {
                parsedValue = constants.Any;
            }
            else if (uint.TryParse(id, out uint linkId))
            {
                parsedValue = linkId;
            }
        }

        #endregion

        #region EnsureCreated Helper

        public static void EnsureCreated<TLinkAddress>(ILinks<TLinkAddress> links, params TLinkAddress[] addresses) where TLinkAddress : IUnsignedNumber<TLinkAddress>
        {
            EnsureCreated(links, links.Create, addresses);
        }

        public static void EnsureCreated<TLinkAddress>(ILinks<TLinkAddress> links, Func<TLinkAddress> creator, params TLinkAddress[] addresses) where TLinkAddress : IUnsignedNumber<TLinkAddress>
        {
            var uInt64ToAddressConverter = CheckedConverter<TLinkAddress, TLinkAddress>.Default;
            var nonExistentAddresses = new HashSet<TLinkAddress>(addresses.Where(x => !links.Exists(x)));
            if (nonExistentAddresses?.Count > 0)
            {
                var max = nonExistentAddresses.Max()!;
                max = uInt64ToAddressConverter.Convert(TLinkAddress.CreateTruncating(Math.Min(ulong.CreateTruncating(max), ulong.CreateTruncating(links.Constants.InternalReferencesRange.Maximum))));
                var createdLinks = new List<TLinkAddress>();
                TLinkAddress createdLink;
                do
                {
                    createdLink = creator();
                    createdLinks.Add(createdLink);
                }
                while (!createdLink.Equals(max));
                for (var i = 0; i < createdLinks.Count; i++)
                {
                    if (!nonExistentAddresses.Contains(createdLinks[i]))
                    {
                        links.Delete(createdLinks[i]);
                    }
                }
            }
        }

        #endregion

        #region Pattern Representation

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

        private static Pattern PatternFromLino(ILinks<uint> links, LinoLink lino)
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

        #endregion
    }
}