using System.Globalization;
using System.Numerics;
using System.Text;
using Link.Foundation.Links.Notation;
using LinoLink = Link.Foundation.Links.Notation.Link<string>;

namespace Foundation.Data.Doublets.Cli;

/// <summary>
/// Preserves reference kinds across the notation parser, whose string model drops
/// delimiters. Names have an internal envelope until they reach a name lookup/write;
/// addresses, variables and wildcards retain their query syntax. Every name is
/// enveloped, including names containing the envelope itself, so there are no collisions.
/// </summary>
internal static class QueryParser
{
    internal static string Name(string identifier) =>
        identifier.Length >= 2 && identifier[0] == '"' && identifier[^1] == '"'
            ? identifier[1..^1] : identifier;

    // Stored transformations must retain the name/address distinction when replayed.
    internal static string FormatIdentifier(string identifier)
    {
        var name = Name(identifier);
        if (name == identifier) return identifier;
        var safeBareName = name.Length > 0 && !uint.TryParse(name, out var ignoredNumber)
            && name != "*" && !name.StartsWith('$') && !name.StartsWith('#')
            && !name.StartsWith('~')
            && !name.Any(character => char.IsWhiteSpace(character) || char.IsControl(character)
                || "():\"'`".Contains(character));
        if (safeBareName) return name;
        if (name.Length == 0 || name == "*" || name.StartsWith('$')) return ReferenceLiteral.Encode(name);
        // Preserve established trigger text for ordinary quoted names: stored
        // trigger identity compares this text, including when removing one.
        return LinoDatabaseOutput.EscapeReference(name);
    }

    internal static IList<LinoLink> Parse(string query) => Parse<uint>(query);

    internal static IList<LinoLink> Parse<T>(string query, QueryLimits? limits = null)
        where T : struct, IBinaryInteger<T>, IUnsignedNumber<T>, IMinMaxValue<T>
    {
        limits?.Input(query);
        var parser = limits is null ? new Parser() : new Parser(true, limits.MaxDepth);
        // Validate the original first so diagnostics retain the user's line/column.
        var original = parser.Parse(query);
        limits?.Nodes(original);
        if (original.Count == 0) return original;

        var text = Comments.StripComments(query);
        var rewritten = new StringBuilder();
        for (var position = 0; position < text.Length;)
        {
            var current = text[position];
            if (" \t\r\n(:)".Contains(current))
            {
                rewritten.Append(current);
                position++;
                continue;
            }

            var start = position;
            var value = string.Empty;
            var quoted = (current is '\'' or '"' or '`') && TryReadQuoted(text, ref position, out value);
            if (!quoted)
            {
                while (position < text.Length && !" \t\r\n(:)".Contains(text[position])) position++;
                value = text[start..position];
                var brace = value.IndexOf('{');
                if (value.StartsWith('~') && brace > 1 && value[1..brace].All(char.IsAsciiDigit))
                {
                    value = ReferenceLiteral.Decode(value);
                    quoted = true;
                }
            }

            if (!quoted) limits?.Numeric(value);
            var isSyntax = !quoted && (T.TryParse(value, NumberStyles.Integer, CultureInfo.InvariantCulture, out var ignoredNumber) || value == "*" || value.StartsWith('$'));
            rewritten.Append(isSyntax ? value : ReferenceLiteral.Encode("\"" + value + "\""));
        }
        return parser.Parse(rewritten.ToString());
    }

    // Match the notation library's N-delimiter rules, including doubled delimiters
    // and the empty-reference fallback for an even opening run. Backslashes are literal.
    private static bool TryReadQuoted(string text, ref int position, out string reference)
    {
        var start = position;
        var quote = text[position];
        while (position < text.Length && text[position] == quote) position++;
        var count = position - start;
        var delimiter = new string(quote, count);
        var escaped = new string(quote, count * 2);
        var value = new StringBuilder();
        while (position < text.Length)
        {
            if (text.AsSpan(position).StartsWith(escaped, StringComparison.Ordinal))
            {
                value.Append(delimiter);
                position += escaped.Length;
            }
            else if (text.AsSpan(position).StartsWith(delimiter, StringComparison.Ordinal)
                && (position + count == text.Length || text[position + count] != quote))
            {
                if (count % 2 == 0 && !IsSubstantive(value.ToString())) break;
                position += count;
                reference = value.ToString();
                return true;
            }
            else
            {
                value.Append(text[position++]);
            }
        }
        // An even run can be an empty name. An unmatched odd prefix falls back
        // to a bare word in the notation grammar, with the quote in its name.
        position = start + (count % 2 == 0 ? count : 0);
        reference = string.Empty;
        return count % 2 == 0;
    }

    private static bool IsSubstantive(string text)
    {
        var depth = 0;
        var visible = false;
        foreach (var character in text)
        {
            if (character == '(') depth++;
            if (character == ')' && --depth < 0) return false;
            if (!char.IsWhiteSpace(character)) visible = true;
        }
        return visible && depth == 0;
    }
}
