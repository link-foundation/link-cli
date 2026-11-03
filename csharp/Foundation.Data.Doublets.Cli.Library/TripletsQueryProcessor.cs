using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Numerics;
using Platform.Data;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;
using LinoParser = Link.Foundation.Links.Notation.Parser;

namespace Foundation.Data.Doublets.Cli
{
    /// <summary>
    /// One change a query made: a creation has no <c>Before</c>, a deletion no
    /// <c>After</c>, and a read reports the same triplet on both sides.
    /// </summary>
    public readonly record struct TripletChange<TLinkAddress>(Triplet<TLinkAddress>? Before, Triplet<TLinkAddress>? After)
        where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>;

    /// <summary>
    /// Substitution queries over triplets: <c>((restriction) (substitution))</c>,
    /// where every pattern has three parts, <c>(index: type from to)</c> or
    /// <c>(type from to)</c> (issue #151). The Rust counterpart is
    /// <c>link_cli::process_triplets_query</c>.
    /// </summary>
    /// <remarks>
    /// <list type="bullet">
    /// <item>A restriction pattern matches triplets; the patterns are a conjunction
    /// over their variables (<c>$name</c>), and <c>*</c> matches anything.</item>
    /// <item>A substitution pattern whose index is the index of a matched triplet
    /// updates it, keeping its address; <c>*</c> in a part keeps that part.</item>
    /// <item>A matched triplet no substitution pattern names is deleted, with the
    /// triplets that use it.</item>
    /// <item>Any other substitution pattern creates a triplet: at its address, under
    /// its name, bound to its variable, or anywhere when it has no index.</item>
    /// </list>
    /// So <c>((i: T f t)) ((i: T f t2))</c> moves the to-part of <c>i</c>,
    /// <c>(() ((T f t)))</c> creates, <c>(((i: T f t)) ())</c> deletes and a query
    /// whose substitution repeats its restriction only reads.
    /// </remarks>
    public static class TripletsQueryProcessor
    {
        private const string QueryShape = "A triplets query is ((restriction) (substitution)).";

        private static readonly string[] PartNames = { "type", "from", "to" };

        /// <summary>
        /// Runs a substitution query and returns the changes it made, in order:
        /// creations and updates as the substitution lists them, then deletions.
        /// </summary>
        public static IReadOnlyList<TripletChange<TLinkAddress>> ProcessQuery<TLinkAddress>(ITriplets<TLinkAddress> triplets, string query)
            where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
        {
            ArgumentNullException.ThrowIfNull(triplets);
            var parsed = new LinoParser().Parse(query ?? string.Empty);
            LinoLink restriction;
            LinoLink substitution;
            if (parsed.Count == 0)
            {
                return Array.Empty<TripletChange<TLinkAddress>>();
            }
            if (parsed.Count == 1)
            {
                var only = parsed[0];
                if (only.Id is not null || only.Values is not { Count: 2 })
                {
                    throw new ArgumentException(QueryShape, nameof(query));
                }
                restriction = only.Values[0];
                substitution = only.Values[1];
            }
            else if (parsed.Count == 2)
            {
                restriction = parsed[0];
                substitution = parsed[1];
            }
            else
            {
                throw new ArgumentException(QueryShape, nameof(query));
            }
            var restrictions = PatternsOf(restriction);
            var substitutions = PatternsOf(substitution);
            var solutions = new List<(Dictionary<string, TLinkAddress> Bindings, List<TLinkAddress> Matched)>();
            if (restrictions.Count == 0)
            {
                solutions.Add((new Dictionary<string, TLinkAddress>(), new List<TLinkAddress>()));
            }
            else
            {
                Solve(triplets, restrictions, 0, new Dictionary<string, TLinkAddress>(), new List<TLinkAddress>(), solutions);
            }
            var changes = new List<TripletChange<TLinkAddress>>();
            var deletes = new List<TLinkAddress>();
            foreach (var (bindings, matched) in solutions)
            {
                var kept = Substitute(triplets, substitutions, bindings, changes).ToHashSet();
                foreach (var index in matched)
                {
                    if (!kept.Contains(index) && !deletes.Contains(index))
                    {
                        deletes.Add(index);
                    }
                }
            }
            foreach (var index in deletes)
            {
                if (triplets.IsTriplet(index))
                {
                    foreach (var deleted in triplets.DeleteTriplet(index))
                    {
                        changes.Add(new TripletChange<TLinkAddress>(deleted, null));
                    }
                }
            }
            return changes;
        }

        internal enum ReferenceKind
        {
            Any,
            Variable,
            Address,
            Name,
        }

        /// <summary>A reference in a pattern: <c>*</c>, <c>$name</c>, an address or a name.</summary>
        internal readonly record struct Reference(ReferenceKind Kind, string Text)
        {
            public static Reference Parse(string text)
            {
                if (text == "*")
                {
                    return new Reference(ReferenceKind.Any, text);
                }
                if (text.StartsWith('$'))
                {
                    return new Reference(ReferenceKind.Variable, text);
                }
                if (uint.TryParse(text, NumberStyles.None, CultureInfo.InvariantCulture, out _))
                {
                    return new Reference(ReferenceKind.Address, text);
                }
                return new Reference(ReferenceKind.Name, text);
            }

            public TLinkAddress Address<TLinkAddress>()
                where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
            {
                return TLinkAddress.Parse(Text, NumberStyles.None, CultureInfo.InvariantCulture);
            }
        }

        /// <summary><c>(index: type from to)</c>, the index optional.</summary>
        internal sealed record Pattern(Reference? Index, Reference[] Parts);

        /// <summary>The patterns of one side of a query: <c>()</c> has none, <c>(i: T f t)</c> is one and <c>((…) (…))</c> is a list of them.</summary>
        internal static List<Pattern> PatternsOf(LinoLink link)
        {
            var values = link.Values ?? (IList<LinoLink>)Array.Empty<LinoLink>();
            if (link.Id is not null || values.Any(value => !HasValues(value)))
            {
                return new List<Pattern> { PatternOf(link) };
            }
            return values.Select(PatternOf).ToList();
        }

        /// <summary>The pattern <c>(index: type from to)</c> or <c>(type from to)</c>.</summary>
        internal static Pattern PatternOf(LinoLink link)
        {
            var values = link.Values ?? (IList<LinoLink>)Array.Empty<LinoLink>();
            if (values.Count != 3)
            {
                throw new ArgumentException($"A triplet pattern has three parts, type, from and to: {Describe(link)} has {values.Count}.");
            }
            var parts = new Reference[3];
            for (var i = 0; i < 3; i++)
            {
                var value = values[i];
                if (value.Id is null || HasValues(value))
                {
                    throw new ArgumentException($"A part of a triplet pattern is a reference, not {Describe(value)}.");
                }
                parts[i] = Reference.Parse(value.Id);
            }
            return new Pattern(link.Id is null ? null : Reference.Parse(link.Id), parts);
        }

        internal static bool HasValues(LinoLink link) => link.Values is { Count: > 0 };

        private static string Describe(LinoLink link)
        {
            var values = string.Join(" ", (link.Values ?? Array.Empty<LinoLink>()).Select(Describe));
            if (link.Id is null)
            {
                return $"({values})";
            }
            return HasValues(link) ? $"({link.Id}: {values})" : link.Id;
        }

        private static TLinkAddress? NameOf<TLinkAddress>(ITriplets<TLinkAddress> triplets, string name)
            where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
        {
            var id = triplets.GetByName(name);
            return id == TLinkAddress.Zero ? null : id;
        }

        private enum RestrictionKind
        {
            Any,
            Value,
            Impossible,
        }

        private static (RestrictionKind Kind, TLinkAddress Value) RestrictionOf<TLinkAddress>(ITriplets<TLinkAddress> triplets, Reference reference, Dictionary<string, TLinkAddress> bindings)
            where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
        {
            switch (reference.Kind)
            {
                case ReferenceKind.Variable:
                    return bindings.TryGetValue(reference.Text, out var bound) ? (RestrictionKind.Value, bound) : (RestrictionKind.Any, default);
                case ReferenceKind.Address:
                    return (RestrictionKind.Value, reference.Address<TLinkAddress>());
                case ReferenceKind.Name:
                    return NameOf(triplets, reference.Text) is { } id ? (RestrictionKind.Value, id) : (RestrictionKind.Impossible, default);
                default:
                    return (RestrictionKind.Any, default);
            }
        }

        private static bool Bind<TLinkAddress>(Dictionary<string, TLinkAddress> bindings, Reference reference, TLinkAddress value)
            where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
        {
            if (reference.Kind != ReferenceKind.Variable)
            {
                return true;
            }
            if (bindings.TryGetValue(reference.Text, out var bound))
            {
                return bound == value;
            }
            bindings[reference.Text] = value;
            return true;
        }

        /// <summary>Every assignment of the variables under which each restriction pattern matches a triplet, with the triplets they matched.</summary>
        private static void Solve<TLinkAddress>(
            ITriplets<TLinkAddress> triplets,
            List<Pattern> patterns,
            int at,
            Dictionary<string, TLinkAddress> bindings,
            List<TLinkAddress> matched,
            List<(Dictionary<string, TLinkAddress> Bindings, List<TLinkAddress> Matched)> solutions)
            where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
        {
            if (at == patterns.Count)
            {
                solutions.Add((bindings, matched.ToList()));
                return;
            }
            var pattern = patterns[at];
            var references = new[] { pattern.Index ?? new Reference(ReferenceKind.Any, "*"), pattern.Parts[0], pattern.Parts[1], pattern.Parts[2] };
            var values = new TLinkAddress?[4];
            for (var slot = 0; slot < 4; slot++)
            {
                var (kind, value) = RestrictionOf(triplets, references[slot], bindings);
                if (kind == RestrictionKind.Impossible)
                {
                    return;
                }
                values[slot] = kind == RestrictionKind.Value ? value : null;
            }
            var query = new TripletQuery<TLinkAddress>(values[0], values[1], values[2], values[3]);
            foreach (var triplet in triplets.EachTriplet(query))
            {
                var next = new Dictionary<string, TLinkAddress>(bindings);
                var found = new[] { triplet.Index, triplet.Type, triplet.From, triplet.To };
                var all = true;
                for (var slot = 0; slot < 4 && all; slot++)
                {
                    all = Bind(next, references[slot], found[slot]);
                }
                if (all)
                {
                    matched.Add(triplet.Index);
                    Solve(triplets, patterns, at + 1, next, matched, solutions);
                    matched.RemoveAt(matched.Count - 1);
                }
            }
        }

        private enum TargetKind
        {
            Existing,
            Address,
            New,
        }

        /// <summary>
        /// Applies the substitution patterns under <paramref name="bindings"/>: updates the
        /// triplets they name, creates the others, and returns the index each pattern wrote,
        /// in the order of the patterns.
        /// </summary>
        /// <remarks>
        /// Every triplet to create is reserved first as a point <c>(i: i i)</c>, which is a
        /// triplet, so the patterns may refer to each other in any order; then each gets its
        /// parts. Every reference is checked before anything is written.
        /// </remarks>
        internal static List<TLinkAddress> Substitute<TLinkAddress>(
            ITriplets<TLinkAddress> triplets,
            List<Pattern> patterns,
            Dictionary<string, TLinkAddress> bindings,
            List<TripletChange<TLinkAddress>> changes)
            where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
        {
            var zero = TLinkAddress.Zero;
            var targets = new List<(TargetKind Kind, TLinkAddress Index, Reference? Reference)>(patterns.Count);
            var definedNames = new HashSet<string>();
            var definedVariables = new HashSet<string>();
            var definedAddresses = new HashSet<TLinkAddress>();
            foreach (var pattern in patterns)
            {
                (TargetKind Kind, TLinkAddress Index, Reference? Reference) target;
                if (pattern.Index is not { } index)
                {
                    target = (TargetKind.New, zero, null);
                }
                else if (index.Kind == ReferenceKind.Any)
                {
                    throw new ArgumentException("A substitution cannot create or update the index *.");
                }
                else if (index.Kind == ReferenceKind.Variable)
                {
                    if (bindings.TryGetValue(index.Text, out var bound))
                    {
                        target = (TargetKind.Existing, bound, null);
                    }
                    else
                    {
                        definedVariables.Add(index.Text);
                        target = (TargetKind.New, zero, index);
                    }
                }
                else if (index.Kind == ReferenceKind.Name)
                {
                    if (NameOf(triplets, index.Text) is { } id)
                    {
                        target = (TargetKind.Existing, id, null);
                    }
                    else
                    {
                        definedNames.Add(index.Text);
                        target = (TargetKind.New, zero, index);
                    }
                }
                else
                {
                    var address = index.Address<TLinkAddress>();
                    if (address == zero)
                    {
                        throw new ArgumentException("A triplet cannot be at address 0.");
                    }
                    if (triplets.IsTriplet(address))
                    {
                        target = (TargetKind.Existing, address, null);
                    }
                    else if (triplets.Exists(address))
                    {
                        throw new InvalidOperationException($"Link {address} is a doublet, not a triplet.");
                    }
                    else
                    {
                        definedAddresses.Add(address);
                        target = (TargetKind.Address, address, null);
                    }
                }
                if (target.Kind == TargetKind.Existing && !triplets.IsTriplet(target.Index))
                {
                    throw new InvalidOperationException($"Link {target.Index} is not a triplet.");
                }
                targets.Add(target);
            }
            for (var i = 0; i < patterns.Count; i++)
            {
                for (var position = 0; position < 3; position++)
                {
                    var part = patterns[i].Parts[position];
                    var known = part.Kind switch
                    {
                        ReferenceKind.Any => targets[i].Kind == TargetKind.Existing,
                        ReferenceKind.Variable => bindings.ContainsKey(part.Text) || definedVariables.Contains(part.Text),
                        ReferenceKind.Name => NameOf(triplets, part.Text) is not null || definedNames.Contains(part.Text),
                        _ => part.Address<TLinkAddress>() is var address && (address == zero
                            ? position > 0
                            : triplets.IsTriplet(address) || definedAddresses.Contains(address)),
                    };
                    if (!known)
                    {
                        throw new ArgumentException($"The {PartNames[position]} part {part.Text} of a substitution is not a triplet.");
                    }
                }
            }

            var indices = new List<TLinkAddress>(targets.Count);
            var created = new HashSet<TLinkAddress>();
            foreach (var target in targets)
            {
                TLinkAddress index;
                if (target.Kind == TargetKind.Existing)
                {
                    index = target.Index;
                }
                else if (target.Kind == TargetKind.Address)
                {
                    index = target.Index;
                    if (!triplets.IsTriplet(index))
                    {
                        triplets.EnsureCreated(index);
                        triplets.Update(new[] { index }, new[] { index, index, index }, null);
                        created.Add(index);
                    }
                }
                else
                {
                    // A name or a variable defined twice is one triplet.
                    TLinkAddress? known = target.Reference switch
                    {
                        { Kind: ReferenceKind.Name } name => NameOf(triplets, name.Text),
                        { Kind: ReferenceKind.Variable } variable => bindings.TryGetValue(variable.Text, out var bound) ? bound : null,
                        _ => null,
                    };
                    if (known is { } existing)
                    {
                        index = existing;
                    }
                    else
                    {
                        index = triplets.CreateTripletPoint();
                        if (target.Reference is { Kind: ReferenceKind.Name } name)
                        {
                            triplets.SetName(index, name.Text);
                        }
                        else if (target.Reference is { Kind: ReferenceKind.Variable } variable)
                        {
                            bindings[variable.Text] = index;
                        }
                        created.Add(index);
                    }
                }
                indices.Add(index);
            }

            var kept = new HashSet<TLinkAddress>();
            for (var i = 0; i < patterns.Count; i++)
            {
                var index = indices[i];
                var before = triplets.GetTriplet(index);
                var current = before ?? new Triplet<TLinkAddress>(index, index, zero, zero);
                var currents = new[] { current.Type, current.From, current.To };
                var parts = new TLinkAddress[3];
                for (var position = 0; position < 3; position++)
                {
                    var part = patterns[i].Parts[position];
                    parts[position] = part.Kind switch
                    {
                        ReferenceKind.Any => currents[position],
                        ReferenceKind.Variable => bindings[part.Text],
                        ReferenceKind.Address => part.Address<TLinkAddress>(),
                        _ => NameOf(triplets, part.Text) ?? throw new InvalidOperationException($"The name {part.Text} is not a triplet."),
                    };
                }
                triplets.UpdateTriplet(index, parts[0], parts[1], parts[2]);
                var after = triplets.GetTriplet(index);
                var reported = created.Contains(index) ? null : before;
                // A triplet two patterns write is reported once, as it ends up.
                var previous = kept.Contains(index)
                    ? changes.FindLastIndex(change => change.After is { } written && written.Index == index)
                    : -1;
                if (previous >= 0)
                {
                    changes[previous] = changes[previous] with { After = after };
                }
                else
                {
                    changes.Add(new TripletChange<TLinkAddress>(reported, after));
                }
                kept.Add(index);
            }
            return indices;
        }
    }
}
