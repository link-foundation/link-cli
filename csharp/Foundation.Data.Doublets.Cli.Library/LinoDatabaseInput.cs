using Platform.Data;
using Platform.Data.Doublets;
using System.Text;
using System.Text.RegularExpressions;
using ReferenceLiteral = Link.Foundation.Links.Notation.ReferenceLiteral;

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
        // Finish parsing and validating every line before touching either store.
        // Enumerating ParseDefinitions while importing leaves a partial import
        // behind when a later line is malformed.
        var definitions = ParseDefinitions(linksNotation).ToArray();
        var context = new ImportContext();

        foreach (var definition in definitions)
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

    private static uint ResolveReference(INamedTypesLinks<uint> links, ImportContext context, ImportReference reference)
    {
        var identifier = reference.Value;
        if (identifier.Length == 0)
        {
            throw new FormatException("LiNo import references must have a value.");
        }

        return !reference.IsQuoted && TryParseSupportedReference(links, identifier, allowNull: true, out var link)
            ? link
            : EnsureNamedPointLink(links, context, identifier);
    }

    private static uint ResolveIndex(INamedTypesLinks<uint> links, ImportContext context, ImportReference reference)
    {
        var identifier = reference.Value;
        if (!reference.IsQuoted && TryParseSupportedReference(links, identifier, allowNull: false, out var link))
        {
            if (!links.Exists(link))
            {
                LinksExtensions.EnsureCreated(links, link);
            }

            return link;
        }

        return EnsureNamedPointLink(links, context, identifier);
    }

    internal static void UpdateLink(INamedTypesLinks<uint> links, uint index, uint source, uint target)
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
        return new ImportParser(linksNotation).Parse();
    }

    private sealed class ImportParser
    {
        private readonly string _text;
        private int _position;

        public ImportParser(string text)
        {
            _text = text;
        }

        public IEnumerable<ImportDefinition> Parse()
        {
            while (true)
            {
                // Physical newlines separate definitions only outside quoted names.
                while (_position < _text.Length && char.IsWhiteSpace(_text[_position]))
                {
                    _position++;
                }

                if (_position == _text.Length)
                {
                    yield break;
                }

                yield return ParseDefinition();
            }
        }

        private ImportDefinition ParseDefinition()
        {
            SkipWhitespace();
            Expect('(');
            var index = ReadReference(stopAtColon: true);
            SkipWhitespace();
            Expect(':');
            var source = ReadReference(stopAtColon: false);
            var target = ReadReference(stopAtColon: false);
            if (source.Value.Length == 0 || target.Value.Length == 0)
            {
                throw Error("LiNo import references must have a value.");
            }
            SkipWhitespace();
            Expect(')');
            SkipWhitespace();

            if (_position < _text.Length && _text[_position] is not '\r' and not '\n')
            {
                throw Error("Unexpected trailing content.");
            }

            return new ImportDefinition(index, source, target);
        }

        private ImportReference ReadReference(bool stopAtColon)
        {
            SkipWhitespace();
            if (_position >= _text.Length)
            {
                throw Error("Expected reference.");
            }

            var first = _text[_position];
            if (first is '\'' or '"' or '`')
            {
                return new ImportReference(ReadQuotedReference(first), IsQuoted: true);
            }

            var start = _position;
            while (_position < _text.Length)
            {
                var current = _text[_position];
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

            var token = _text[start.._position];
            if (Regex.IsMatch(token, @"^~[0-9]+\{"))
            {
                try
                {
                    // Literals are names even when their decoded text is numeric.
                    return new ImportReference(ReferenceLiteral.Decode(token), IsQuoted: true);
                }
                catch (FormatException error)
                {
                    throw Error(error.Message);
                }
            }

            return new ImportReference(token, IsQuoted: false);
        }

        private string ReadQuotedReference(char quote)
        {
            _position++;
            var value = new StringBuilder();

            while (_position < _text.Length)
            {
                var current = _text[_position++];
                if (current == quote)
                {
                    return value.ToString();
                }

                // Like LiNo quoted references, backslashes are literal, including
                // a trailing backslash immediately before the closing quote.
                value.Append(current);
            }

            throw Error("Unterminated quoted reference.");
        }

        private void Expect(char expected)
        {
            SkipWhitespace();
            if (_position >= _text.Length || _text[_position] != expected)
            {
                throw Error($"Expected '{expected}'.");
            }

            _position++;
        }

        private void SkipWhitespace()
        {
            while (_position < _text.Length && char.IsWhiteSpace(_text[_position])
                && _text[_position] is not '\r' and not '\n')
            {
                _position++;
            }
        }

        private FormatException Error(string message)
        {
            var lineNumber = 1 + _text.AsSpan(0, _position).Count('\n');
            return new FormatException($"Invalid LiNo import line {lineNumber}: {message}");
        }
    }

    // Keep quoting information so numeric names never become addresses.
    private sealed record ImportReference(string Value, bool IsQuoted);

    private sealed record ImportDefinition(ImportReference Index, ImportReference Source, ImportReference Target);

    private sealed class ImportContext
    {
        public Dictionary<string, uint> NamedReferences { get; } = new(StringComparer.Ordinal);
    }
}
