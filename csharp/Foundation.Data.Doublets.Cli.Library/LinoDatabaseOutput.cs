using System.Text;
using System.Text.RegularExpressions;
using ReferenceLiteral = Link.Foundation.Links.Notation.ReferenceLiteral;
using Platform.Data;
using Platform.Data.Doublets;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli;

public static class LinoDatabaseOutput
{
    private static readonly Regex NumberTokenRegex = new(@"(?<![\w$])\d+(?![\w$])", RegexOptions.Compiled);

    public static IReadOnlyList<string> FormatDatabase(INamedTypesLinks<uint> links)
    {
        var any = links.Constants.Any;
        var query = new DoubletLink(index: any, source: any, target: any);

        return links
            .All(query)
            .Select(link => new DoubletLink(link))
            .OrderBy(link => link.Index)
            .Select(link => FormatLink(links, link))
            .ToList();
    }

    public static void WriteDatabase(INamedTypesLinks<uint> links, TextWriter writer)
    {
        foreach (var line in FormatDatabase(links))
        {
            writer.WriteLine(line);
        }
    }

    public static void WriteToFile(INamedTypesLinks<uint> links, string path)
    {
        using var writer = new StreamWriter(path, append: false);
        WriteDatabase(links, writer);
    }

    public static string FormatLink(INamedTypesLinks<uint> links, DoubletLink link)
    {
        return $"({FormatReference(links, link.Index)}: {FormatReference(links, link.Source)} {FormatReference(links, link.Target)})";
    }

    public static string FormatChange(INamedTypesLinks<uint> links, DoubletLink linkBefore, DoubletLink linkAfter)
    {
        var beforeText = linkBefore.IsNull() ? "" : FormatLink(links, linkBefore);
        var afterText = linkAfter.IsNull() ? "" : FormatLink(links, linkAfter);
        return $"({beforeText}) ({afterText})";
    }

    public static string FormatStructure(INamedTypesLinks<uint> links, uint linkId)
    {
        var visited = new HashSet<uint>();
        var frames = new List<DoubletLink>();
        string terminal;
        while (true)
        {
            if (!links.Exists(linkId)) throw new InvalidOperationException($"Link '{linkId}' does not exist.");
            visited.Add(linkId);
            var link = new DoubletLink(links.GetLink(linkId));
            frames.Add(link);
            if (links.Exists(link.Source) && !visited.Contains(link.Source)) { linkId = link.Source; continue; }
            terminal = FormatReference(links, link.Source);
            break;
        }
        // Preserve deepest-first target/index name lookups, including errors.
        var parts = new List<(string Index, string Target)>(frames.Count);
        for (int index = frames.Count - 1; index >= 0; index--)
        {
            var link = frames[index];
            var target = FormatReference(links, link.Target);
            parts.Add((FormatReference(links, link.Index), target));
        }
        var output = new StringBuilder();
        for (int index = parts.Count - 1; index >= 0; index--)
            output.Append('(').Append(parts[index].Index).Append(": ");
        output.Append(terminal);
        foreach (var part in parts) output.Append(' ').Append(part.Target).Append(')');
        return output.ToString();
    }

    public static string Namify(INamedTypesLinks<uint> namedLinks, string linksNotation)
    {
        return NumberTokenRegex.Replace(linksNotation, match =>
        {
            var numberLink = uint.Parse(match.Value);
            var name = namedLinks.GetName(numberLink);
            return name is null ? match.Value : EscapeReference(name);
        });
    }

    private static string FormatReference(INamedTypesLinks<uint> links, uint link)
    {
        var name = links.GetName(link);
        return name is null ? link.ToString() : EscapeReference(name);
    }

    internal static string EscapeReference(string reference)
    {
        if (string.IsNullOrEmpty(reference))
        {
            return string.Empty;
        }

        var hasSingleQuote = reference.Contains('\'');
        var hasDoubleQuote = reference.Contains('"');
        if ((hasSingleQuote && hasDoubleQuote) || reference.Any(character => character < ' ' || character == '\u007f'))
        {
            // Share the notation library's lossless form for controls and mixed
            // quotes, instead of inventing backslash escapes for literal names.
            return ReferenceLiteral.Encode(reference);
        }

        var needsQuoting = uint.TryParse(reference, out _)
            || reference.Contains(':')
            || reference.Contains('(')
            || reference.Contains(')')
            || reference.Any(char.IsWhiteSpace)
            || reference.Contains('\\')
            || reference.Contains('`')
            || reference.StartsWith('#')
            || Regex.IsMatch(reference, @"^~[0-9]+\{")
            || hasSingleQuote
            || hasDoubleQuote;

        if (hasDoubleQuote)
        {
            return $"'{reference}'";
        }

        if (hasSingleQuote)
        {
            return $"\"{reference}\"";
        }

        return needsQuoting ? $"'{reference}'" : reference;
    }
}
