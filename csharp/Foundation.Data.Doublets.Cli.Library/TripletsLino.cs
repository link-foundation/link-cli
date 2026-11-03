using System;
using System.Collections.Generic;
using System.Numerics;
using System.Text;
using LinoParser = Link.Foundation.Links.Notation.Parser;

namespace Foundation.Data.Doublets.Cli
{
    /// <summary>
    /// Triplets in Links Notation: one <c>(index: type from to)</c> per line (issue #151).
    /// </summary>
    /// <remarks>
    /// The export lists every triplet in the order of its address, a part by its name
    /// when it has one and <c>0</c> for an empty from or to. The import reads the same
    /// lines back: a numeric index is kept, a named index is the triplet of that name
    /// (created when there is none), and a line without an index is a new triplet.
    /// Lines may refer to each other in any order, since every triplet is reserved
    /// before any of them gets its parts. The Rust counterparts are
    /// <c>link_cli::triplets_to_lino</c> and <c>link_cli::import_triplets_lino</c>.
    /// </remarks>
    public static class TripletsLino
    {
        /// <summary>Every triplet as a line <c>(index: type from to)</c>, in the order of the addresses.</summary>
        public static string Export<TLinkAddress>(ITriplets<TLinkAddress> triplets)
            where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
        {
            ArgumentNullException.ThrowIfNull(triplets);
            var text = new StringBuilder();
            foreach (var triplet in triplets.EachTriplet(TripletQuery<TLinkAddress>.Any))
            {
                text.Append('(')
                    .Append(Reference(triplets, triplet.Index)).Append(": ")
                    .Append(Reference(triplets, triplet.Type)).Append(' ')
                    .Append(Reference(triplets, triplet.From)).Append(' ')
                    .Append(Reference(triplets, triplet.To)).Append(")\n");
            }
            return text.ToString();
        }

        /// <summary>Writes the triplets of <paramref name="text"/> and returns their indices, one per line.</summary>
        public static IReadOnlyList<TLinkAddress> Import<TLinkAddress>(ITriplets<TLinkAddress> triplets, string text)
            where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
        {
            ArgumentNullException.ThrowIfNull(triplets);
            var parsed = new LinoParser().Parse(text ?? string.Empty);
            var patterns = new List<TripletsQueryProcessor.Pattern>(parsed.Count);
            foreach (var parsedLine in parsed)
            {
                var line = parsedLine;
                // A line wrapped once more is that line.
                if (line.Id is null && line.Values is { Count: 1 } && TripletsQueryProcessor.HasValues(line.Values[0]))
                {
                    line = line.Values[0];
                }
                var pattern = TripletsQueryProcessor.PatternOf(line);
                var references = new List<TripletsQueryProcessor.Reference>(pattern.Parts);
                if (pattern.Index is { } index)
                {
                    references.Add(index);
                }
                foreach (var reference in references)
                {
                    if (reference.Kind is TripletsQueryProcessor.ReferenceKind.Any or TripletsQueryProcessor.ReferenceKind.Variable)
                    {
                        throw new ArgumentException("A triplet in Links Notation has no variables or wildcards.", nameof(text));
                    }
                }
                patterns.Add(pattern);
            }
            var changes = new List<TripletChange<TLinkAddress>>();
            return TripletsQueryProcessor.Substitute(triplets, patterns, new Dictionary<string, TLinkAddress>(), changes);
        }

        private static string Reference<TLinkAddress>(ITriplets<TLinkAddress> triplets, TLinkAddress id)
            where TLinkAddress : struct, IUnsignedNumber<TLinkAddress>
        {
            if (id == TLinkAddress.Zero)
            {
                return "0";
            }
            return triplets.GetName(id) is { } name ? LinoDatabaseOutput.EscapeReference(name) : id.ToString()!;
        }
    }
}
