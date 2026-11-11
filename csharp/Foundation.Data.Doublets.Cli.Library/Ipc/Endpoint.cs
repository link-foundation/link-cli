using System.Globalization;
using System.Text;

namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>A parsed endpoint string, <c>[kind:]address[?name=value&amp;...]</c> (issue #140).</summary>
/// <remarks>
/// Text without a known kind prefix is a TCP <c>host:port</c>, so every address accepted before IPC endpoints
/// existed keeps its meaning. The grammar and the options of every kind are those of
/// <c>docs/protocol/ipc-transports-v1.md</c>.
/// </remarks>
public sealed class Endpoint : IEquatable<Endpoint>
{
    private static readonly string[] Common = ["timeout"];
    private static readonly string[] RendezvousOptions = ["poll", "accept-poll"];
    private static readonly string[] RingOptions = ["capacity"];
    private static readonly string[] DatagramOptions = ["chunk", "rto"];

    private readonly SortedDictionary<string, string> _options;

    private Endpoint(string kind, string address, SortedDictionary<string, string> options)
    {
        Kind = kind;
        Address = address;
        _options = options;
    }

    /// <summary>The kind, lowercase, such as <c>tcp</c> or <c>shm</c>.</summary>
    public string Kind { get; }

    /// <summary>Everything between the kind and the options.</summary>
    public string Address { get; }

    /// <summary>Every option, sorted by name.</summary>
    public IReadOnlyDictionary<string, string> Options => _options;

    /// <summary>The raw value of an option, or null.</summary>
    public string? Option(string name) => _options.TryGetValue(name, out var value) ? value : null;

    /// <summary>Parses an endpoint string.</summary>
    /// <exception cref="IpcException">The text is not a valid endpoint.</exception>
    public static Endpoint Parse(string text)
    {
        ArgumentNullException.ThrowIfNull(text);
        text = text.Trim();
        if (text.Length == 0)
        {
            throw IpcException.Invalid("an endpoint must not be empty");
        }
        var colon = text.IndexOf(':', StringComparison.Ordinal);
        string kind;
        string rest;
        if (colon >= 0 && IsKind(text[..colon]))
        {
            kind = text[..colon].ToLowerInvariant();
            rest = text[(colon + 1)..];
        }
        else if (colon < 0 && IsKind(text))
        {
            kind = text.ToLowerInvariant();
            rest = "";
        }
        else
        {
            kind = "tcp";
            rest = text;
        }
        if (kind == "exec")
        {
            if (rest.Trim().Length == 0)
            {
                throw IpcException.Invalid("`exec:` needs a command");
            }
            return new Endpoint(kind, rest, new SortedDictionary<string, string>(StringComparer.Ordinal));
        }
        var question = rest.IndexOf('?', StringComparison.Ordinal);
        var address = question < 0 ? rest : rest[..question];
        var options = new SortedDictionary<string, string>(StringComparer.Ordinal);
        if (question >= 0)
        {
            var builtin = IpcTransports.Kinds.Contains(kind);
            var allowed = OptionsOf(kind);
            foreach (var pair in rest[(question + 1)..].Split('&'))
            {
                if (pair.Length == 0)
                {
                    continue;
                }
                var equals = pair.IndexOf('=', StringComparison.Ordinal);
                if (equals < 0)
                {
                    throw IpcException.Invalid($"endpoint option `{pair}` needs a `=value`");
                }
                var name = pair[..equals].ToLowerInvariant();
                if (builtin && !allowed.Contains(name))
                {
                    var list = allowed.Count == 0 ? "" : $" (options: {string.Join(", ", allowed)})";
                    throw IpcException.Invalid($"`{kind}:` endpoints have no option `{name}`{list}");
                }
                options[name] = pair[(equals + 1)..];
            }
        }
        var endpoint = new Endpoint(kind, address, options);
        endpoint.Validate();
        return endpoint;
    }

    /// <summary>Parses <paramref name="text"/>; false when it is not a valid endpoint.</summary>
    public static bool TryParse(string text, out Endpoint? endpoint)
    {
        try
        {
            endpoint = Parse(text);
            return true;
        }
        catch (IpcException)
        {
            endpoint = null;
            return false;
        }
    }

    private static List<string> OptionsOf(string kind)
    {
        var names = new List<string>(Common);
        switch (kind)
        {
            case "tcp" or "unix" or "stdio" or "exec":
                break;
            case "pipe":
                if (!OperatingSystem.IsWindows())
                {
                    names.AddRange(RendezvousOptions);
                }
                break;
            case "fifo":
                names.AddRange(RendezvousOptions);
                break;
            case "shm" or "mmap" or "file":
                names.AddRange(RendezvousOptions);
                names.AddRange(RingOptions);
                break;
            case "signal":
                names.AddRange(RendezvousOptions);
                names.AddRange(RingOptions);
                names.Add("signo");
                break;
            case "udp":
                names.AddRange(DatagramOptions);
                names.AddRange(["heartbeat", "dead"]);
                break;
            case "mq":
                names.AddRange(RendezvousOptions);
                names.AddRange(DatagramOptions);
                names.Add("maxmsg");
                break;
            case "sysvmq" or "mailslot" or "copydata" or "alpc":
                names.AddRange(RendezvousOptions);
                names.AddRange(DatagramOptions);
                break;
            default:
                return [];
        }
        return names;
    }

    private void Validate()
    {
        if (Kind != "stdio" && Address.Length == 0 && IpcTransports.Kinds.Contains(Kind))
        {
            throw IpcException.Invalid($"`{Kind}:` endpoints need an address");
        }
        if (Kind == "stdio" && Address.Length != 0)
        {
            throw IpcException.Invalid("`stdio` takes no address");
        }
        foreach (var name in new[] { "timeout", "poll", "accept-poll", "heartbeat", "dead", "rto" })
        {
            Millis(name, 1);
        }
        Number("capacity", 65536, 64, 1UL << 30);
        Number("chunk", 4096, 16, 65000);
        Number("maxmsg", 8, 1, 65536);
        Number("signo", 12, 1, 64);
    }

    /// <summary>A numeric option within <paramref name="min"/>..=<paramref name="max"/>, or <paramref name="fallback"/>.</summary>
    public ulong Number(string name, ulong fallback, ulong min, ulong max)
    {
        if (Option(name) is not { } text)
        {
            return fallback;
        }
        var digits = text.StartsWith('+') ? text[1..] : text;
        if (digits.Length > 0
            && digits.All(char.IsAsciiDigit)
            && ulong.TryParse(digits, NumberStyles.None, CultureInfo.InvariantCulture, out var value)
            && value >= min && value <= max)
        {
            return value;
        }
        throw IpcException.Invalid($"endpoint option `{name}` must be a whole number from {min} to {max}, found `{text}`");
    }

    /// <summary>A duration option given in milliseconds, or <paramref name="fallback"/> milliseconds.</summary>
    public TimeSpan Millis(string name, ulong fallback) =>
        TimeSpan.FromMilliseconds(Number(name, fallback, 0, 86_400_000));

    /// <summary>The connect and handshake timeout (<c>timeout</c>, 10 s by default), at least 1 ms.</summary>
    public TimeSpan Timeout
    {
        get
        {
            TimeSpan timeout;
            try
            {
                timeout = Millis("timeout", 10_000);
            }
            catch (IpcException)
            {
                timeout = TimeSpan.FromSeconds(10);
            }
            return timeout < TimeSpan.FromMilliseconds(1) ? TimeSpan.FromMilliseconds(1) : timeout;
        }
    }

    /// <summary>The endpoint as text: <c>kind[:address][?name=value&amp;...]</c>.</summary>
    public override string ToString()
    {
        var text = new StringBuilder(Kind);
        if (Address.Length != 0)
        {
            text.Append(':').Append(Address);
        }
        var separator = '?';
        foreach (var (name, value) in _options)
        {
            text.Append(separator).Append(name).Append('=').Append(value);
            separator = '&';
        }
        return text.ToString();
    }

    /// <inheritdoc/>
    public bool Equals(Endpoint? other) => other is not null && ToString() == other.ToString();

    /// <inheritdoc/>
    public override bool Equals(object? obj) => Equals(obj as Endpoint);

    /// <inheritdoc/>
    public override int GetHashCode() => StringComparer.Ordinal.GetHashCode(ToString());

    private static bool IsKind(string prefix)
    {
        var lower = prefix.ToLowerInvariant();
        return IpcTransports.Kinds.Contains(lower) || IpcTransports.IsRegistered(lower);
    }
}
