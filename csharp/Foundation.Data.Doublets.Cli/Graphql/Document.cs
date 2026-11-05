using System.Globalization;
using System.Text;

namespace Foundation.Data.Doublets.Cli.Graphql;

/// <summary>A value in a GraphQL document: a variable, a literal, a list or an object.</summary>
public abstract class Value
{
    /// <summary>The value with its variables replaced; a variable nobody passed is null.</summary>
    public abstract Json Resolve(Json variables);

    internal sealed class Variable(string name) : Value
    {
        public override Json Resolve(Json variables) => variables.Get(name) ?? Json.Null;
    }

    internal sealed class Literal(Json json) : Value
    {
        public override Json Resolve(Json variables) => json;
    }

    internal sealed class ListOf(List<Value> items) : Value
    {
        public override Json Resolve(Json variables) => Json.Array(items.Select(item => item.Resolve(variables)));
    }

    internal sealed class ObjectOf(List<(string Key, Value Value)> pairs) : Value
    {
        public override Json Resolve(Json variables) =>
            Json.Object(pairs.Select(pair => KeyValuePair.Create(pair.Key, pair.Value.Resolve(variables))));
    }
}

public sealed record Directive(string Name, List<(string Name, Value Value)> Arguments);

public abstract class Selection;

public sealed class Field : Selection
{
    public string? Alias { get; init; }
    public required string Name { get; init; }
    public required List<(string Name, Value Value)> Arguments { get; init; }
    public required List<Directive> Directives { get; init; }
    public required List<Selection> Selections { get; init; }

    public string ResponseKey => Alias ?? Name;

    /// <summary>The argument's value; absent and null are the same.</summary>
    public Json? Argument(string name, Json variables)
    {
        foreach (var (key, value) in Arguments)
        {
            if (key == name)
            {
                var resolved = value.Resolve(variables);
                return resolved.IsNull ? null : resolved;
            }
        }
        return null;
    }
}

public sealed class Spread(string name, List<Directive> directives) : Selection
{
    public string Name { get; } = name;
    public List<Directive> Directives { get; } = directives;
}

public sealed class Inline(string? on, List<Directive> directives, List<Selection> selections) : Selection
{
    public string? On { get; } = on;
    public List<Directive> Directives { get; } = directives;
    public List<Selection> Selections { get; } = selections;
}

public enum OperationKind { Query, Mutation, Subscription }

/// <param name="Kind">Query, mutation or subscription.</param>
/// <param name="Name">The operation's name, if any.</param>
/// <param name="Variables">Variable name and default value.</param>
/// <param name="Selections">What the operation selects.</param>
public sealed record Operation(OperationKind Kind, string? Name, List<(string Name, Value? Default)> Variables, List<Selection> Selections);

/// <summary>GraphQL documents: operations, selections, fragments and values.</summary>
public sealed class Document
{
    /// <summary>Deep enough for any selection a client means.</summary>
    private const int MaxDepth = 64;

    private enum TokenKind { Punctuator, Name, Int, Float, String }

    public List<Operation> Operations { get; } = [];
    public Dictionary<string, List<Selection>> Fragments { get; } = [];

    /// <exception cref="FormatException">The text is not a document.</exception>
    public static Document Parse(string text)
    {
        var parser = new Parser(Tokenize(text));
        var document = new Document();
        while (parser.Peek is not null)
        {
            if (parser.EatName("fragment"))
            {
                var name = parser.Name();
                if (!parser.EatName("on"))
                {
                    throw new FormatException($"Expected 'on' in fragment {name}.");
                }
                parser.Name();
                parser.Directives(0);
                document.Fragments[name] = parser.SelectionSet(0);
            }
            else
            {
                document.Operations.Add(parser.ReadOperation());
            }
        }
        if (document.Operations.Count == 0)
        {
            throw new FormatException("A document has at least one operation.");
        }
        return document;
    }

    /// <exception cref="FormatException">No operation is the one asked for.</exception>
    public Operation OperationNamed(string? name)
    {
        if (name is not null)
        {
            return Operations.Find(operation => operation.Name == name)
                ?? throw new FormatException($"Unknown operation named '{name}'.");
        }
        return Operations.Count == 1
            ? Operations[0]
            : throw new FormatException("Must provide operation name if query contains multiple operations.");
    }

    /// <summary>A token as the Rust parser's errors print it, so that both say the same.</summary>
    private static string Debug(Token? token)
    {
        if (token is null)
        {
            return "None";
        }
        var inner = token.Kind switch
        {
            TokenKind.Punctuator => $"Punctuator('{token.Text}')",
            TokenKind.Name => $"Name(\"{token.Text}\")",
            TokenKind.Int => $"Int({token.Int.ToString(CultureInfo.InvariantCulture)})",
            TokenKind.Float => $"Float({DebugFloat(token.Float)})",
            _ => $"String(\"{token.Text.Replace("\\", "\\\\").Replace("\"", "\\\"").Replace("\n", "\\n").Replace("\r", "\\r").Replace("\t", "\\t")}\")",
        };
        return $"Some({inner})";
    }

    private static string DebugFloat(double value)
    {
        var text = Json.WriteFloat(value);
        return text.Contains('.') ? text : text + ".0";
    }

    private static List<Token> Tokenize(string text)
    {
        var tokens = new List<Token>();
        var at = 0;
        while (at < text.Length)
        {
            var character = text[at];
            switch (character)
            {
                case ' ' or '\n' or '\r' or '\t' or ',' or '\uFEFF':
                    at++;
                    break;
                case '#':
                    while (at < text.Length && text[at] != '\n')
                    {
                        at++;
                    }
                    break;
                case '!' or '$' or '(' or ')' or ':' or '=' or '@' or '[' or ']' or '{' or '}' or '|' or '&':
                    tokens.Add(new Token(TokenKind.Punctuator, character.ToString()));
                    at++;
                    break;
                case '.':
                    if (at + 2 >= text.Length || text[at + 1] != '.' || text[at + 2] != '.')
                    {
                        throw new FormatException("Unexpected '.' in the document.");
                    }
                    tokens.Add(new Token(TokenKind.Punctuator, "."));
                    at += 3;
                    break;
                case '"':
                    at = StringToken(text, at, tokens);
                    break;
                case '-' or (>= '0' and <= '9'):
                    at = NumberToken(text, at, tokens);
                    break;
                case '_' or (>= 'a' and <= 'z') or (>= 'A' and <= 'Z'):
                {
                    var start = at;
                    while (at < text.Length && (text[at] == '_' || char.IsAsciiLetterOrDigit(text[at])))
                    {
                        at++;
                    }
                    tokens.Add(new Token(TokenKind.Name, text[start..at]));
                    break;
                }
                default:
                    var shown = Rune.TryGetRuneAt(text, at, out var rune) ? rune.ToString() : character.ToString();
                    throw new FormatException($"Unexpected character '{shown}' in the document.");
            }
        }
        return tokens;
    }

    /// <summary>Reads the number at <paramref name="at"/> and returns where it ends.</summary>
    private static int NumberToken(string text, int at, List<Token> tokens)
    {
        var start = at++;
        while (at < text.Length && text[at] is (>= '0' and <= '9') or '.' or 'e' or 'E' or '+' or '-')
        {
            at++;
        }
        var literal = text[start..at];
        if (long.TryParse(literal, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var integer))
        {
            tokens.Add(new Token(TokenKind.Int, Int: integer));
        }
        else if (double.TryParse(literal, NumberStyles.Float, CultureInfo.InvariantCulture, out var number))
        {
            tokens.Add(new Token(TokenKind.Float, Float: number));
        }
        else
        {
            throw new FormatException($"Invalid number '{literal}'.");
        }
        return at;
    }

    /// <summary>Reads the string at <paramref name="at"/> and returns where it ends.</summary>
    private static int StringToken(string text, int at, List<Token> tokens)
    {
        if (at + 2 < text.Length && text[at + 1] == '"' && text[at + 2] == '"')
        {
            at += 3;
            var start = at;
            while (true)
            {
                if (at + 2 >= text.Length)
                {
                    throw new FormatException("Unterminated block string.");
                }
                if (text[at] == '"' && text[at + 1] == '"' && text[at + 2] == '"')
                {
                    tokens.Add(new Token(TokenKind.String, text[start..at]));
                    return at + 3;
                }
                at++;
            }
        }
        var begin = at++;
        while (at < text.Length && text[at] != '"')
        {
            at += text[at] == '\\' ? 2 : 1;
        }
        if (at >= text.Length)
        {
            throw new FormatException("Unterminated string.");
        }
        at++;
        tokens.Add(new Token(TokenKind.String, Json.Parse(text[begin..at]).StringValue));
        return at;
    }

    private sealed record Token(TokenKind Kind, string Text = "", long Int = 0, double Float = 0)
    {
        public bool Is(char punctuator) => Kind == TokenKind.Punctuator && Text[0] == punctuator;
    }

    private sealed class Parser(List<Token> tokens)
    {
        private int _at;

        public Token? Peek => _at < tokens.Count ? tokens[_at] : null;

        public bool EatName(string name)
        {
            if (Peek is { Kind: TokenKind.Name } found && found.Text == name)
            {
                _at++;
                return true;
            }
            return false;
        }

        public string Name()
        {
            if (Peek is { Kind: TokenKind.Name } found)
            {
                _at++;
                return found.Text;
            }
            throw new FormatException($"Expected a name, found {Debug(Peek)}.");
        }

        public Operation ReadOperation()
        {
            if (Peek?.Is('{') == true)
            {
                return new Operation(OperationKind.Query, null, [], SelectionSet(0));
            }
            var keyword = Name();
            var kind = keyword switch
            {
                "query" => OperationKind.Query,
                "mutation" => OperationKind.Mutation,
                "subscription" => OperationKind.Subscription,
                _ => throw new FormatException($"Unknown operation type '{keyword}'."),
            };
            var name = Peek is { Kind: TokenKind.Name } ? Name() : null;
            var variables = new List<(string, Value?)>();
            if (Eat('('))
            {
                while (!Eat(')'))
                {
                    Expect('$');
                    var variable = Name();
                    Expect(':');
                    SkipType(0);
                    var defaultValue = Eat('=') ? ReadValue(0) : null;
                    Directives(0);
                    variables.Add((variable, defaultValue));
                }
            }
            Directives(0);
            return new Operation(kind, name, variables, SelectionSet(0));
        }

        public List<Selection> SelectionSet(int depth)
        {
            if (depth > MaxDepth)
            {
                throw new FormatException("The selection is nested too deeply.");
            }
            Expect('{');
            var selections = new List<Selection>();
            while (!Eat('}'))
            {
                if (Eat('.'))
                {
                    if (EatName("on"))
                    {
                        var on = Name();
                        var directives = Directives(depth);
                        selections.Add(new Inline(on, directives, SelectionSet(depth + 1)));
                    }
                    else if (Peek is { Kind: TokenKind.Name })
                    {
                        var name = Name();
                        selections.Add(new Spread(name, Directives(depth)));
                    }
                    else
                    {
                        var directives = Directives(depth);
                        selections.Add(new Inline(null, directives, SelectionSet(depth + 1)));
                    }
                }
                else
                {
                    var first = Name();
                    string? alias = null;
                    var name = first;
                    if (Eat(':'))
                    {
                        alias = first;
                        name = Name();
                    }
                    var arguments = Arguments(depth);
                    var directives = Directives(depth);
                    var inner = Peek?.Is('{') == true ? SelectionSet(depth + 1) : [];
                    selections.Add(new Field { Alias = alias, Name = name, Arguments = arguments, Directives = directives, Selections = inner });
                }
            }
            return selections;
        }

        public List<Directive> Directives(int depth)
        {
            var directives = new List<Directive>();
            while (Eat('@'))
            {
                var name = Name();
                directives.Add(new Directive(name, Arguments(depth)));
            }
            return directives;
        }

        private bool Eat(char punctuator)
        {
            if (Peek?.Is(punctuator) == true)
            {
                _at++;
                return true;
            }
            return false;
        }

        private void Expect(char punctuator)
        {
            if (!Eat(punctuator))
            {
                throw new FormatException($"Expected '{punctuator}', found {Debug(Peek)}.");
            }
        }

        /// <summary>The depth guard is here as a stack overflow would end the process.</summary>
        private void SkipType(int depth)
        {
            if (depth > MaxDepth)
            {
                throw new FormatException("The type is nested too deeply.");
            }
            if (Eat('['))
            {
                SkipType(depth + 1);
                Expect(']');
            }
            else
            {
                Name();
            }
            Eat('!');
        }

        private List<(string, Value)> Arguments(int depth)
        {
            var arguments = new List<(string, Value)>();
            if (Eat('('))
            {
                while (!Eat(')'))
                {
                    var name = Name();
                    Expect(':');
                    arguments.Add((name, ReadValue(depth)));
                }
            }
            return arguments;
        }

        private Value ReadValue(int depth)
        {
            if (depth > MaxDepth)
            {
                throw new FormatException("The value is nested too deeply.");
            }
            var token = Peek;
            _at++;
            switch (token)
            {
                case { Kind: TokenKind.Int }:
                    return new Value.Literal(Json.Int(token.Int));
                case { Kind: TokenKind.Float }:
                    return new Value.Literal(Json.Float(token.Float));
                case { Kind: TokenKind.String }:
                    return new Value.Literal(Json.Str(token.Text));
                case { Kind: TokenKind.Name }:
                    return new Value.Literal(token.Text switch
                    {
                        "true" => Json.Bool(true),
                        "false" => Json.Bool(false),
                        "null" => Json.Null,
                        // An enum value travels as its name, as JSON variables do.
                        _ => Json.Str(token.Text),
                    });
            }
            if (token?.Is('$') == true)
            {
                return new Value.Variable(Name());
            }
            if (token?.Is('[') == true)
            {
                var items = new List<Value>();
                while (!Eat(']'))
                {
                    items.Add(ReadValue(depth + 1));
                }
                return new Value.ListOf(items);
            }
            if (token?.Is('{') == true)
            {
                var pairs = new List<(string, Value)>();
                while (!Eat('}'))
                {
                    var name = Name();
                    Expect(':');
                    pairs.Add((name, ReadValue(depth + 1)));
                }
                return new Value.ObjectOf(pairs);
            }
            throw new FormatException($"Expected a value, found {Debug(token)}.");
        }
    }
}
