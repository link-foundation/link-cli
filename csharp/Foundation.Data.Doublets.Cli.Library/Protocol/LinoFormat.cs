using Link.Foundation.Links.Notation;

using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>
/// Canonical LiNo text for documents, shared by every protocol.
/// </summary>
/// <remarks>
/// A document is the list of top-level links of a message. A reference is a
/// <see cref="LinoLink"/> whose <c>Values</c> is null. The canonical model is
/// the one the Rust links-notation parser produces, so documents decoded from
/// the same bytes compare equal in both ports; the C# parser keeps one extra
/// wrapper around single-reference groups, which <see cref="ParseDocument"/>
/// removes. <c>ParseDocument(FormatDocument(document))</c> always equals
/// <c>document</c>, which is what makes the text and binary protocols
/// interchangeable.
/// </remarks>
public static class LinoFormat
{
    private static readonly char[] Quotes = { '\'', '"', '`' };

    /// <summary>A reference: a link without values.</summary>
    public static LinoLink Reference(string text) => new(text);

    /// <summary>A link with an optional id and the given values.</summary>
    public static LinoLink Link(string? id, IList<LinoLink> values) => new(id!, values);

    /// <summary>True when <paramref name="link"/> is a reference.</summary>
    public static bool IsReference(LinoLink link) => link.Values is null;

    /// <summary>Parses LiNo text into a canonical document. Blank input is the empty document.</summary>
    public static IReadOnlyList<LinoLink> ParseDocument(string text)
    {
        ArgumentNullException.ThrowIfNull(text);
        if (string.IsNullOrWhiteSpace(text))
        {
            return Array.Empty<LinoLink>();
        }
        IList<LinoLink> parsed;
        try
        {
            parsed = new Parser().Parse(text);
        }
        catch (Exception exception) when (exception is not OutOfMemoryException)
        {
            throw new LinoProtocolException(LinoProtocolErrorKind.InvalidLino, exception.Message, exception);
        }
        return parsed.Select(Canonical).ToList();
    }

    /// <summary>
    /// Converts a link produced by the C# parser into the canonical model:
    /// an unnamed group holding exactly one reference is that reference.
    /// </summary>
    public static LinoLink Canonical(LinoLink link)
    {
        if (link.Values is null)
        {
            return link;
        }
        if (link.Id is null && link.Values.Count == 1 && link.Values[0].Values is null)
        {
            return link.Values[0];
        }
        return Link(link.Id, link.Values.Select(Canonical).ToList());
    }

    /// <summary>
    /// Formats a document as canonical LiNo text, one top-level link per line.
    /// A top-level link without an id and with at least two values is written
    /// without its outer parentheses: <c>() ((1 1))</c>.
    /// </summary>
    public static string FormatDocument(IEnumerable<LinoLink> document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return string.Join("\n", document.Select(FormatTopLevel));
    }

    private static string FormatTopLevel(LinoLink link) =>
        link.Values is { Count: >= 2 } values && link.Id is null ? JoinValues(values) : FormatLink(link);

    /// <summary>Formats one link as it appears nested inside another link.</summary>
    public static string FormatLink(LinoLink link)
    {
        if (link.Values is not { } values)
        {
            return FormatReference(link.Id ?? string.Empty);
        }
        if (link.Id is null)
        {
            // `(a)` parses back as the reference `a`, so a one-reference
            // link needs a second pair of parentheses.
            return values.Count == 1 && values[0].Values is null
                ? $"(({FormatReference(values[0].Id ?? string.Empty)}))"
                : $"({JoinValues(values)})";
        }
        return values.Count == 0
            ? $"({FormatReference(link.Id)}:)"
            : $"({FormatReference(link.Id)}: {JoinValues(values)})";
    }

    private static string JoinValues(IEnumerable<LinoLink> values) => string.Join(" ", values.Select(FormatLink));

    /// <summary>Quotes a reference when it would not survive parsing as a bare word.</summary>
    /// <remarks>
    /// links-notation opens a quoted reference with a run of N equal quote
    /// characters, closes it with the next run of exactly N, and reads 2N
    /// quotes inside as N literal ones. The opening run is counted greedily,
    /// so the chosen quote must differ from the first character; N is one
    /// more than the longest run of that quote inside, and odd, because an
    /// even delimiter run may be read as an empty reference.
    /// </remarks>
    public static string FormatReference(string reference)
    {
        ArgumentNullException.ThrowIfNull(reference);
        var needsQuotes = reference.Length == 0
            || reference.Any(character => char.IsWhiteSpace(character) || character is '(' or ')' or ':' or '\'' or '"' or '`');
        if (!needsQuotes)
        {
            return reference;
        }
        var (quote, count) = Quotes
            .Where(quote => reference.Length == 0 || reference[0] != quote)
            .Select(quote => (Quote: quote, Count: (LongestRun(reference, quote) + 1) | 1))
            .MinBy(candidate => candidate.Count);
        var delimiter = new string(quote, count);
        return $"{delimiter}{reference}{delimiter}";
    }

    private static int LongestRun(string text, char quote)
    {
        var (longest, current) = (0, 0);
        foreach (var character in text)
        {
            current = character == quote ? current + 1 : 0;
            longest = Math.Max(longest, current);
        }
        return longest;
    }
}
