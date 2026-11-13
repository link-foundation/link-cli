using System.Globalization;
using System.Text;
using System.Text.Json;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>
/// The flat JSON objects of the rendezvous files: string and non-negative integer members; members of other types
/// are skipped. Written with sorted names, as the Rust implementation writes them.
/// </summary>
internal sealed class IpcJson
{
    private static readonly JsonDocumentOptions Reading = new() { MaxDepth = 34 };
    private readonly SortedDictionary<string, object> _members = new(StringComparer.Ordinal);

    public IpcJson With(string name, string value)
    {
        _members[name] = value;
        return this;
    }

    public IpcJson With(string name, ulong value)
    {
        _members[name] = value;
        return this;
    }

    /// <summary>Copies every member of <paramref name="other"/> into this object.</summary>
    public IpcJson WithAll(IpcJson other)
    {
        foreach (var (name, value) in other._members)
        {
            _members[name] = value;
        }
        return this;
    }

    public string? Text(string name) => _members.TryGetValue(name, out var value) ? value as string : null;

    public ulong? Number(string name) => _members.TryGetValue(name, out var value) && value is ulong number ? number : null;

    public ulong RequireNumber(string name) =>
        Number(name) ?? throw IpcException.InvalidData($"the handshake lacks the number `{name}`");

    public string ToJson()
    {
        var output = new StringBuilder("{");
        var first = true;
        foreach (var (name, value) in _members)
        {
            if (!first)
            {
                output.Append(',');
            }
            first = false;
            WriteString(output, name);
            output.Append(':');
            if (value is string text)
            {
                WriteString(output, text);
            }
            else
            {
                output.Append(((ulong)value).ToString(CultureInfo.InvariantCulture));
            }
        }
        return output.Append('}').ToString();
    }

    public override string ToString() => ToJson();

    public static IpcJson Parse(string text)
    {
        JsonDocument document;
        try
        {
            document = JsonDocument.Parse(text, Reading);
        }
        catch (JsonException error)
        {
            throw new IpcException(IpcErrorKind.InvalidData, error.Message, error);
        }
        using (document)
        {
            if (document.RootElement.ValueKind != JsonValueKind.Object)
            {
                throw IpcException.InvalidData("expected `{` at offset 0 of the JSON object");
            }
            var parsed = new IpcJson();
            foreach (var member in document.RootElement.EnumerateObject())
            {
                switch (member.Value.ValueKind)
                {
                    case JsonValueKind.String:
                        parsed._members[member.Name] = member.Value.GetString()!;
                        break;
                    case JsonValueKind.Number when IsWhole(member.Value.GetRawText()) && member.Value.TryGetUInt64(out var number):
                        parsed._members[member.Name] = number;
                        break;
                    default:
                        // A member of another type, or a number of another kind, is skipped.
                        parsed._members.Remove(member.Name);
                        break;
                }
            }
            return parsed;
        }
    }

    /// <summary>Parses a JSON array of strings, such as the argument vector of <c>exec:</c>.</summary>
    public static List<string> ParseStrings(string text)
    {
        try
        {
            using var document = JsonDocument.Parse(text, Reading);
            if (document.RootElement.ValueKind != JsonValueKind.Array)
            {
                throw IpcException.InvalidData("expected `[` at offset 0 of the JSON object");
            }
            var strings = new List<string>();
            foreach (var element in document.RootElement.EnumerateArray())
            {
                strings.Add(element.ValueKind == JsonValueKind.String
                    ? element.GetString()!
                    : throw IpcException.InvalidData("expected a JSON string"));
            }
            return strings;
        }
        catch (JsonException error)
        {
            throw new IpcException(IpcErrorKind.InvalidData, error.Message, error);
        }
    }

    private static bool IsWhole(string raw) => raw.All(char.IsAsciiDigit);

    private static void WriteString(StringBuilder output, string text)
    {
        output.Append('"');
        foreach (var character in text)
        {
            switch (character)
            {
                case '"': output.Append("\\\""); break;
                case '\\': output.Append("\\\\"); break;
                case '\n': output.Append("\\n"); break;
                case '\r': output.Append("\\r"); break;
                case '\t': output.Append("\\t"); break;
                case < ' ': output.Append(CultureInfo.InvariantCulture, $"\\u{(int)character:x4}"); break;
                default: output.Append(character); break;
            }
        }
        output.Append('"');
    }
}
