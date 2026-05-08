using Platform.Data;
using Platform.Data.Doublets;
using System.Text;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli;

public static class LinoDatabaseInput
{
    public static void ReadFromFile(INamedTypesLinks<uint> links, string path)
    {
        ImportText(links, File.ReadAllText(path));
    }

    public static void ImportText(INamedTypesLinks<uint> links, string linksNotation)
    {
        var context = new ImportContext();

        foreach (var definition in ParseDefinitions(linksNotation))
        {
            ImportLink(links, context, definition);
        }
    }

    private static void ImportLink(INamedTypesLinks<uint> links, ImportContext context, ImportDefinition definition)
    {
        var source = ResolveReference(links, context, definition.Source);
        var target = ResolveReference(links, context, definition.Target);
        var index = ResolveIndex(links, context, definition.Index);
        UpdateLink(links, index, source, target);
    }

    private static uint ResolveReference(INamedTypesLinks<uint> links, ImportContext context, string identifier)
    {
        if (identifier.Length == 0)
        {
            throw new FormatException("LiNo import references must have a value.");
        }

        return TryParseSupportedReference(links, identifier, allowNull: true, out var link)
            ? link
            : EnsureNamedPointLink(links, context, identifier);
    }

    private static uint ResolveIndex(INamedTypesLinks<uint> links, ImportContext context, string identifier)
    {
        if (TryParseSupportedReference(links, identifier, allowNull: false, out var link))
        {
            if (!links.Exists(link))
            {
                LinksExtensions.EnsureCreated(links, link);
            }

            return link;
        }

        return EnsureNamedPointLink(links, context, identifier);
    }

    private static void UpdateLink(INamedTypesLinks<uint> links, uint index, uint source, uint target)
    {
        if (!links.Exists(index))
        {
            LinksExtensions.EnsureCreated(links, index);
        }

        var current = new DoubletLink(links.GetLink(index));
        if (current.Source == source && current.Target == target)
        {
            return;
        }

        links.Update(
            new DoubletLink(index, links.Constants.Any, links.Constants.Any),
            new DoubletLink(index, source, target),
            (_, _) => links.Constants.Continue);
    }

    private static uint EnsureNamedPointLink(INamedTypesLinks<uint> links, ImportContext context, string name)
    {
        if (context.NamedReferences.TryGetValue(name, out var known))
        {
            return known;
        }

        var existing = links.GetByName(name);
        if (existing != links.Constants.Null)
        {
            context.NamedReferences[name] = existing;
            return existing;
        }

        existing = FindByExistingName(links, name);
        if (existing != links.Constants.Null)
        {
            context.NamedReferences[name] = existing;
            return existing;
        }

        var link = links.CreateAndUpdate(links.Constants.Null, links.Constants.Null);
        links.SetName(link, name);
        links.Update(
            new DoubletLink(link, links.Constants.Null, links.Constants.Null),
            new DoubletLink(link, link, link),
            (_, _) => links.Constants.Continue);
        context.NamedReferences[name] = link;
        return link;
    }

    private static uint FindByExistingName(INamedTypesLinks<uint> links, string name)
    {
        var any = links.Constants.Any;
        var query = new DoubletLink(index: any, source: any, target: any);

        foreach (var link in links.All(query))
        {
            var doublet = new DoubletLink(link);
            if (links.GetName(doublet.Index) == name)
            {
                return doublet.Index;
            }
        }

        return links.Constants.Null;
    }

    private static bool TryParseSupportedReference(INamedTypesLinks<uint> links, string identifier, bool allowNull, out uint link)
    {
        if (!uint.TryParse(identifier, out link))
        {
            return false;
        }

        if (allowNull && link == links.Constants.Null)
        {
            return true;
        }

        return link != links.Constants.Null && link <= links.Constants.InternalReferencesRange.Maximum;
    }

    private static IEnumerable<ImportDefinition> ParseDefinitions(string linksNotation)
    {
        var lineNumber = 0;
        foreach (var rawLine in linksNotation.Split('\n'))
        {
            lineNumber++;
            var line = rawLine.Trim();
            if (line.Length == 0)
            {
                continue;
            }

            yield return new LineParser(line, lineNumber).Parse();
        }
    }

    private sealed class LineParser
    {
        private readonly string _line;
        private readonly int _lineNumber;
        private int _position;

        public LineParser(string line, int lineNumber)
        {
            _line = line;
            _lineNumber = lineNumber;
        }

        public ImportDefinition Parse()
        {
            SkipWhitespace();
            Expect('(');
            var index = ReadReference(stopAtColon: true);
            SkipWhitespace();
            Expect(':');
            var source = ReadReference(stopAtColon: false);
            var target = ReadReference(stopAtColon: false);
            SkipWhitespace();
            Expect(')');
            SkipWhitespace();

            if (_position != _line.Length)
            {
                throw Error("Unexpected trailing content.");
            }

            return new ImportDefinition(index, source, target);
        }

        private string ReadReference(bool stopAtColon)
        {
            SkipWhitespace();
            if (_position >= _line.Length)
            {
                throw Error("Expected reference.");
            }

            var first = _line[_position];
            if (first is '\'' or '"')
            {
                return ReadQuotedReference(first);
            }

            var start = _position;
            while (_position < _line.Length)
            {
                var current = _line[_position];
                if (char.IsWhiteSpace(current) || current == ')' || (stopAtColon && current == ':'))
                {
                    break;
                }

                _position++;
            }

            if (_position == start)
            {
                throw Error("Expected reference.");
            }

            return _line[start.._position];
        }

        private string ReadQuotedReference(char quote)
        {
            _position++;
            var value = new StringBuilder();

            while (_position < _line.Length)
            {
                var current = _line[_position++];
                if (current == quote)
                {
                    return value.ToString();
                }

                if (current == '\\' && _position < _line.Length)
                {
                    value.Append(_line[_position++]);
                    continue;
                }

                value.Append(current);
            }

            throw Error("Unterminated quoted reference.");
        }

        private void Expect(char expected)
        {
            SkipWhitespace();
            if (_position >= _line.Length || _line[_position] != expected)
            {
                throw Error($"Expected '{expected}'.");
            }

            _position++;
        }

        private void SkipWhitespace()
        {
            while (_position < _line.Length && char.IsWhiteSpace(_line[_position]))
            {
                _position++;
            }
        }

        private FormatException Error(string message)
        {
            return new FormatException($"Invalid LiNo import line {_lineNumber}: {message}");
        }
    }

    private sealed record ImportDefinition(string Index, string Source, string Target);

    private sealed class ImportContext
    {
        public Dictionary<string, uint> NamedReferences { get; } = new(StringComparer.Ordinal);
    }
}
