using Platform.Data;
using Platform.Data.Doublets;
using System.Text;
using System.Text.RegularExpressions;
using ReferenceLiteral = Link.Foundation.Links.Notation.ReferenceLiteral;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli;

public static class LinoDatabaseInput
{
    /// <summary>The default nesting limit for a LiNo text import.</summary>
    public const int DefaultImportDepth = 64;

    /// <summary>The highest nesting limit supported by the recursive import path.</summary>
    public const int MaxImportDepth = 128;

    /// <summary>Imports a LiNo file with a nesting limit of 64 levels.</summary>
    public static void ReadFromFile(INamedTypesLinks<uint> links, string path)
    {
        ReadFromFile(links, path, DefaultImportDepth);
    }

    /// <summary>Imports a LiNo file with a nesting limit from 1 to 128.</summary>
    public static void ReadFromFile(INamedTypesLinks<uint> links, string path, int maxDepth)
    {
        ImportText(links, File.ReadAllText(path), maxDepth);
    }

    /// <summary>Imports LiNo text with a nesting limit of 64 levels.</summary>
    public static void ImportText(INamedTypesLinks<uint> links, string linksNotation)
    {
        ImportText(links, linksNotation, DefaultImportDepth);
    }

    /// <summary>Imports LiNo text with a nesting limit from 1 to 128.</summary>
    public static void ImportText(INamedTypesLinks<uint> links, string linksNotation, int maxDepth)
    {
        if (maxDepth is < 1 or > MaxImportDepth)
        {
            throw new ArgumentOutOfRangeException(nameof(maxDepth), $"LiNo import nesting limit must be an integer from 1 to {MaxImportDepth}.");
        }

        // Finish parsing and validating every line before touching either store.
        var definitions = new ImportParser(linksNotation, maxDepth).Parse().ToArray();
        var context = new ImportContext();

        foreach (var definition in definitions)
        {
            ImportLink(links, context, definition);
        }
    }

    private static uint ImportLink(INamedTypesLinks<uint> links, ImportContext context, ImportDefinition definition)
    {
        var source = ResolveReference(links, context, definition.Source);
        var target = ResolveReference(links, context, definition.Target);
        if (definition.Index is null)
        {
            return links.GetOrCreate(source, target);
        }

        var index = ResolveIndex(links, context, definition.Index);
        UpdateLink(links, index, source, target);
        return index;
    }

    private static uint ResolveReference(INamedTypesLinks<uint> links, ImportContext context, ImportValue value)
    {
        if (value is ImportDefinition definition)
        {
            return ImportLink(links, context, definition);
        }

        var reference = (ImportReference)value;
        var identifier = reference.Value;
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

        var link = links.CreateAndUpdate(links.Constants.Null, links.Constants.Null);
        links.SetName(link, name);
        links.Update(
            new DoubletLink(link, links.Constants.Null, links.Constants.Null),
            new DoubletLink(link, link, link),
            (_, _) => links.Constants.Continue);
        context.NamedReferences[name] = link;
        return link;
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

    private sealed class ImportParser
    {
        private readonly string _text;
        private readonly int _maxDepth;
        private int _position;

        public ImportParser(string text, int maxDepth)
        {
            _text = text;
            _maxDepth = maxDepth;
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

                var definition = ParseDefinition(1);
                SkipWhitespace();
                if (_position < _text.Length && _text[_position] is not '\r' and not '\n')
                {
                    throw Error("Unexpected trailing content.");
                }

                yield return definition;
            }
        }

        private ImportDefinition ParseDefinition(int depth)
        {
            if (depth > _maxDepth)
            {
                throw Error($"Nesting too deep: nesting depth exceeds the maximum of {_maxDepth}.");
            }

            Expect('(');
            SkipWhitespace();
            ImportReference? index = null;
            ImportReference? firstValue = null;
            if (depth == 1)
            {
                index = ReadReference(stopAtColon: true);
                Expect(':');
            }
            else if (_position < _text.Length && _text[_position] != '(')
            {
                // A nested pair may start with an index followed by a colon.
                firstValue = ReadReference(stopAtColon: true);
                SkipWhitespace();
                if (_position < _text.Length && _text[_position] == ':')
                {
                    index = firstValue;
                    firstValue = null;
                    _position++;
                }
            }

            var source = firstValue is null ? ReadValue(depth) : RequireValue(firstValue);
            var target = ReadValue(depth);
            Expect(')');
            return new ImportDefinition(index, source, target);
        }

        private ImportValue ReadValue(int depth)
        {
            SkipWhitespace();
            if (_position < _text.Length && _text[_position] == '(')
            {
                return ParseDefinition(depth + 1);
            }

            return RequireValue(ReadReference(stopAtColon: false));
        }

        private ImportReference RequireValue(ImportReference reference)
        {
            if (reference.Value.Length == 0)
            {
                throw Error("LiNo import references must have a value.");
            }

            return reference;
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

    private abstract record ImportValue;

    // Keep quoting information so numeric names never become addresses.
    private sealed record ImportReference(string Value, bool IsQuoted) : ImportValue;

    private sealed record ImportDefinition(ImportReference? Index, ImportValue Source, ImportValue Target) : ImportValue;

    private sealed class ImportContext
    {
        public Dictionary<string, uint> NamedReferences { get; } = new(StringComparer.Ordinal);
    }
}
