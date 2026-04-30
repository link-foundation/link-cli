using System.Text.RegularExpressions;
using Platform.Data;
using Platform.Data.Doublets;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli;

public static class LinoDatabaseOutput
{
    private static readonly Regex NumberTokenRegex = new(@"(?<![\w$])\d+(?![\w$])", RegexOptions.Compiled);

    public static IReadOnlyList<string> FormatDatabase(NamedLinksDecorator<uint> links)
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

    public static void WriteDatabase(NamedLinksDecorator<uint> links, TextWriter writer)
    {
        foreach (var line in FormatDatabase(links))
        {
            writer.WriteLine(line);
        }
    }

    public static void WriteToFile(NamedLinksDecorator<uint> links, string path)
    {
        using var writer = new StreamWriter(path, append: false);
        WriteDatabase(links, writer);
    }

    public static string FormatLink(NamedLinksDecorator<uint> links, DoubletLink link)
    {
        return $"({FormatReference(links, link.Index)}: {FormatReference(links, link.Source)} {FormatReference(links, link.Target)})";
    }

    public static string FormatChange(NamedLinksDecorator<uint> links, DoubletLink linkBefore, DoubletLink linkAfter)
    {
        var beforeText = linkBefore.IsNull() ? "" : FormatLink(links, linkBefore);
        var afterText = linkAfter.IsNull() ? "" : FormatLink(links, linkAfter);
        return $"({beforeText}) ({afterText})";
    }

    public static string Namify(NamedLinksDecorator<uint> namedLinks, string linksNotation)
    {
        return NumberTokenRegex.Replace(linksNotation, match =>
        {
            var numberLink = uint.Parse(match.Value);
            var name = namedLinks.GetName(numberLink);
            return name is null ? match.Value : EscapeReference(name);
        });
    }

    private static string FormatReference(NamedLinksDecorator<uint> links, uint link)
    {
        var name = links.GetName(link);
        return name is null ? link.ToString() : EscapeReference(name);
    }

    private static string EscapeReference(string reference)
    {
        if (string.IsNullOrEmpty(reference) || string.IsNullOrWhiteSpace(reference))
        {
            return string.Empty;
        }

        var hasSingleQuote = reference.Contains('\'');
        var hasDoubleQuote = reference.Contains('"');
        var needsQuoting = reference.Contains(':')
            || reference.Contains('(')
            || reference.Contains(')')
            || reference.Contains(' ')
            || reference.Contains('\t')
            || reference.Contains('\n')
            || reference.Contains('\r')
            || hasSingleQuote
            || hasDoubleQuote;

        if (hasSingleQuote && hasDoubleQuote)
        {
            return $"'{reference.Replace("'", "\\'")}'";
        }

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
