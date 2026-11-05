using System.Globalization;
using System.Text;

namespace Foundation.Data.Doublets.Cli.Graphql;

public enum JsonKind { Null, Bool, Int, Float, String, Array, Object }

/// <summary>
/// The JSON of GraphQL over HTTP and WebSocket: a value keeps the order of its
/// object keys, so a response lists its fields in the order they were asked.
/// </summary>
public sealed class Json
{
    public static readonly Json Null = new(JsonKind.Null);

    /// <summary>Deep enough for any request a client means; a deeper one is refused rather than overflowing the stack.</summary>
    private const int MaxDepth = 128;

    private Json(JsonKind kind) => Kind = kind;

    public JsonKind Kind { get; }
    public bool BoolValue { get; private init; }
    public long IntValue { get; private init; }
    public double FloatValue { get; private init; }
    public string StringValue { get; private init; } = "";
    public List<Json> Items { get; private init; } = [];
    public List<KeyValuePair<string, Json>> Pairs { get; private init; } = [];

    public bool IsNull => Kind == JsonKind.Null;

    public static Json Bool(bool value) => new(JsonKind.Bool) { BoolValue = value };
    public static Json Int(long value) => new(JsonKind.Int) { IntValue = value };
    public static Json Float(double value) => new(JsonKind.Float) { FloatValue = value };
    public static Json Str(string value) => new(JsonKind.String) { StringValue = value };
    public static Json Array(IEnumerable<Json> items) => new(JsonKind.Array) { Items = items.ToList() };
    public static Json Object(IEnumerable<KeyValuePair<string, Json>> pairs) => new(JsonKind.Object) { Pairs = pairs.ToList() };

    public static Json Object(params (string Key, Json Value)[] pairs) =>
        new(JsonKind.Object) { Pairs = pairs.Select(pair => KeyValuePair.Create(pair.Key, pair.Value)).ToList() };

    /// <summary>Reads JSON; a malformed text throws <see cref="FormatException"/>.</summary>
    public static Json Parse(string text)
    {
        var reader = new Reader(Encoding.UTF8.GetBytes(text));
        var value = reader.Value(0);
        reader.Space();
        if (reader.At != reader.Bytes.Length)
        {
            throw new FormatException($"Unexpected text after JSON at byte {reader.At}.");
        }
        return value;
    }

    /// <summary>The value of a key, or C# null when there is none (a JSON null is <see cref="Null"/>).</summary>
    public Json? Get(string key)
    {
        if (Kind != JsonKind.Object)
        {
            return null;
        }
        foreach (var pair in Pairs)
        {
            if (pair.Key == key)
            {
                return pair.Value;
            }
        }
        return null;
    }

    public string? AsStr() => Kind == JsonKind.String ? StringValue : null;

    public long? AsI64()
    {
        if (Kind == JsonKind.Int)
        {
            return IntValue;
        }
        if (Kind == JsonKind.Float && FloatValue - Math.Truncate(FloatValue) == 0.0)
        {
            return Saturate(FloatValue);
        }
        return null;
    }

    public double? AsF64() => Kind switch
    {
        JsonKind.Int => IntValue,
        JsonKind.Float => FloatValue,
        _ => null,
    };

    /// <summary>A float as an integer, stopping at the ends of the range, as Rust's cast does.</summary>
    public static long Saturate(double value)
    {
        if (double.IsNaN(value))
        {
            return 0;
        }
        if (value >= 9223372036854775808.0)
        {
            return long.MaxValue;
        }
        return value <= -9223372036854775808.0 ? long.MinValue : (long)value;
    }

    /// <summary>One object or a list of them, as a list: GraphQL coerces a single value to a list of one, and DeepClient relies on it.</summary>
    public List<Json> AsList() => Kind switch
    {
        JsonKind.Array => Items,
        JsonKind.Null => [],
        _ => [this],
    };

    /// <summary>Structural equality: <c>1</c> and <c>1.0</c> differ, and the order of keys matters.</summary>
    public static bool Same(Json left, Json right)
    {
        if (left.Kind != right.Kind)
        {
            return false;
        }
        switch (left.Kind)
        {
            case JsonKind.Null: return true;
            case JsonKind.Bool: return left.BoolValue == right.BoolValue;
            case JsonKind.Int: return left.IntValue == right.IntValue;
            case JsonKind.Float: return left.FloatValue == right.FloatValue;
            case JsonKind.String: return left.StringValue == right.StringValue;
            case JsonKind.Array:
                if (left.Items.Count != right.Items.Count)
                {
                    return false;
                }
                for (var at = 0; at < left.Items.Count; at++)
                {
                    if (!Same(left.Items[at], right.Items[at]))
                    {
                        return false;
                    }
                }
                return true;
            default:
                if (left.Pairs.Count != right.Pairs.Count)
                {
                    return false;
                }
                for (var at = 0; at < left.Pairs.Count; at++)
                {
                    if (left.Pairs[at].Key != right.Pairs[at].Key || !Same(left.Pairs[at].Value, right.Pairs[at].Value))
                    {
                        return false;
                    }
                }
                return true;
        }
    }

    public string Write()
    {
        var builder = new StringBuilder();
        WriteTo(builder);
        return builder.ToString();
    }

    /// <summary>The shortest digits that read back, never with an exponent and with no <c>.0</c>, as Rust displays a float.</summary>
    internal static string WriteFloat(double value)
    {
        var text = value.ToString("R", CultureInfo.InvariantCulture);
        var exponentAt = text.IndexOf('E');
        if (exponentAt < 0)
        {
            return text;
        }
        var exponent = int.Parse(text[(exponentAt + 1)..], NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture);
        var mantissa = text[..exponentAt];
        var sign = mantissa.StartsWith('-') ? "-" : "";
        mantissa = mantissa.TrimStart('-');
        var pointAt = mantissa.IndexOf('.');
        var digits = mantissa.Replace(".", "");
        var scale = (pointAt < 0 ? mantissa.Length : pointAt) + exponent;
        if (scale <= 0)
        {
            return sign + "0." + new string('0', -scale) + digits;
        }
        return scale >= digits.Length
            ? sign + digits + new string('0', scale - digits.Length)
            : sign + digits[..scale] + "." + digits[scale..];
    }

    private void WriteTo(StringBuilder builder)
    {
        switch (Kind)
        {
            case JsonKind.Null:
                builder.Append("null");
                break;
            case JsonKind.Bool:
                builder.Append(BoolValue ? "true" : "false");
                break;
            case JsonKind.Int:
                builder.Append(IntValue.ToString(CultureInfo.InvariantCulture));
                break;
            case JsonKind.Float:
                builder.Append(double.IsFinite(FloatValue) ? WriteFloat(FloatValue) : "null");
                break;
            case JsonKind.String:
                WriteString(StringValue, builder);
                break;
            case JsonKind.Array:
                builder.Append('[');
                for (var at = 0; at < Items.Count; at++)
                {
                    if (at > 0)
                    {
                        builder.Append(',');
                    }
                    Items[at].WriteTo(builder);
                }
                builder.Append(']');
                break;
            default:
                builder.Append('{');
                for (var at = 0; at < Pairs.Count; at++)
                {
                    if (at > 0)
                    {
                        builder.Append(',');
                    }
                    WriteString(Pairs[at].Key, builder);
                    builder.Append(':');
                    Pairs[at].Value.WriteTo(builder);
                }
                builder.Append('}');
                break;
        }
    }

    private static void WriteString(string text, StringBuilder builder)
    {
        builder.Append('"');
        foreach (var character in text)
        {
            switch (character)
            {
                case '"': builder.Append("\\\""); break;
                case '\\': builder.Append("\\\\"); break;
                case '\n': builder.Append("\\n"); break;
                case '\r': builder.Append("\\r"); break;
                case '\t': builder.Append("\\t"); break;
                default:
                    if (character < 0x20)
                    {
                        builder.Append("\\u").Append(((int)character).ToString("x4", CultureInfo.InvariantCulture));
                    }
                    else
                    {
                        builder.Append(character);
                    }
                    break;
            }
        }
        builder.Append('"');
    }

    /// <summary>Works on UTF-8 bytes, so the offsets in its errors are those of the Rust reader.</summary>
    private sealed class Reader(byte[] bytes)
    {
        public readonly byte[] Bytes = bytes;
        public int At;

        private int Current => At < Bytes.Length ? Bytes[At] : -1;

        public void Space()
        {
            while (Current is ' ' or '\n' or '\r' or '\t')
            {
                At++;
            }
        }

        public Json Value(int depth)
        {
            if (depth > MaxDepth)
            {
                throw new FormatException("JSON is nested too deeply.");
            }
            Space();
            switch (Current)
            {
                case 'n': return Literal("null", Null);
                case 't': return Literal("true", Bool(true));
                case 'f': return Literal("false", Bool(false));
                case '"': return Str(ReadString());
                case '[': return ReadArray(depth);
                case '{': return ReadObject(depth);
                case '-' or (>= '0' and <= '9'): return ReadNumber();
                default: throw new FormatException($"Invalid JSON at byte {At}.");
            }
        }

        private Json Literal(string text, Json value)
        {
            if (At + text.Length <= Bytes.Length && Encoding.ASCII.GetString(Bytes, At, text.Length) == text)
            {
                At += text.Length;
                return value;
            }
            throw new FormatException($"Invalid JSON at byte {At}.");
        }

        private Json ReadArray(int depth)
        {
            At++;
            var items = new List<Json>();
            Space();
            if (Current == ']')
            {
                At++;
                return Array(items);
            }
            while (true)
            {
                items.Add(Value(depth + 1));
                Space();
                switch (Current)
                {
                    case ',':
                        At++;
                        break;
                    case ']':
                        At++;
                        return Array(items);
                    default:
                        throw new FormatException($"Invalid JSON array at byte {At}.");
                }
            }
        }

        private Json ReadObject(int depth)
        {
            At++;
            var pairs = new List<KeyValuePair<string, Json>>();
            Space();
            if (Current == '}')
            {
                At++;
                return Object(pairs);
            }
            while (true)
            {
                Space();
                if (Current != '"')
                {
                    throw new FormatException($"Invalid JSON object at byte {At}.");
                }
                var key = ReadString();
                Space();
                if (Current != ':')
                {
                    throw new FormatException($"Invalid JSON object at byte {At}.");
                }
                At++;
                pairs.Add(KeyValuePair.Create(key, Value(depth + 1)));
                Space();
                switch (Current)
                {
                    case ',':
                        At++;
                        break;
                    case '}':
                        At++;
                        return Object(pairs);
                    default:
                        throw new FormatException($"Invalid JSON object at byte {At}.");
                }
            }
        }

        private Json ReadNumber()
        {
            var start = At;
            while (Current is '-' or '+' or '.' or 'e' or 'E' or (>= '0' and <= '9'))
            {
                At++;
            }
            var text = Encoding.ASCII.GetString(Bytes, start, At - start);
            if (long.TryParse(text, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var integer))
            {
                return Int(integer);
            }
            if (double.TryParse(text, NumberStyles.Float, CultureInfo.InvariantCulture, out var number))
            {
                return Float(number);
            }
            throw new FormatException($"Invalid JSON number at byte {start}.");
        }

        private uint Hex4()
        {
            if (At + 4 <= Bytes.Length
                && uint.TryParse(Encoding.ASCII.GetString(Bytes, At, 4), NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture, out var value))
            {
                At += 4;
                return value;
            }
            throw new FormatException($"Invalid JSON escape at byte {At}.");
        }

        private uint Unicode()
        {
            var code = Hex4();
            if (code is >= 0xD800 and < 0xDC00 && At + 1 < Bytes.Length && Bytes[At] == '\\' && Bytes[At + 1] == 'u')
            {
                At += 2;
                var low = Hex4();
                code = 0x10000 + ((code - 0xD800) << 10) + (unchecked(low - 0xDC00) & 0x3FF);
            }
            return code;
        }

        private string ReadString()
        {
            At++;
            var output = new List<byte>();
            Span<byte> buffer = stackalloc byte[4];
            while (true)
            {
                if (At >= Bytes.Length)
                {
                    throw new FormatException("Unterminated JSON string.");
                }
                var current = Bytes[At++];
                if (current == '"')
                {
                    return Encoding.UTF8.GetString(output.ToArray());
                }
                if (current != '\\')
                {
                    output.Add(current);
                    continue;
                }
                if (At >= Bytes.Length)
                {
                    throw new FormatException("Unterminated JSON string.");
                }
                var escape = Bytes[At++];
                uint code = escape switch
                {
                    (byte)'n' => '\n',
                    (byte)'r' => '\r',
                    (byte)'t' => '\t',
                    (byte)'b' => 8u,
                    (byte)'f' => 12u,
                    (byte)'/' => '/',
                    (byte)'\\' => '\\',
                    (byte)'"' => '"',
                    (byte)'u' => Unicode(),
                    _ => throw new FormatException($"Invalid JSON escape at byte {At}."),
                };
                var rune = Rune.IsValid(code) ? new Rune(code) : Rune.ReplacementChar;
                var written = rune.EncodeToUtf8(buffer);
                for (var at = 0; at < written; at++)
                {
                    output.Add(buffer[at]);
                }
            }
        }
    }
}
