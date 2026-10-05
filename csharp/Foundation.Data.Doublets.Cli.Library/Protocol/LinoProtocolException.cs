namespace Foundation.Data.Doublets.Cli.Protocol;

/// <summary>What went wrong while encoding, decoding or exchanging a LiNo message.</summary>
public enum LinoProtocolErrorKind
{
    /// <summary>The underlying transport failed.</summary>
    Io,
    /// <summary>The peer sent bytes that are not a valid message.</summary>
    Malformed,
    /// <summary>The message text is not valid LiNo.</summary>
    InvalidLino,
    /// <summary>The message exceeds one of the configured <see cref="DecodeLimits"/>.</summary>
    LimitExceeded,
    /// <summary>The document cannot be represented with the chosen options.</summary>
    Unencodable,
    /// <summary>The server answered with an <c>(error: …)</c> document.</summary>
    Remote,
}

/// <summary>Raised by the LiNo network protocols (issue #105).</summary>
public sealed class LinoProtocolException : Exception
{
    public LinoProtocolException()
        : this(LinoProtocolErrorKind.Malformed, "unknown protocol error")
    {
    }

    public LinoProtocolException(string message)
        : this(LinoProtocolErrorKind.Malformed, message)
    {
    }

    public LinoProtocolException(string message, Exception innerException)
        : this(LinoProtocolErrorKind.Io, message, innerException)
    {
    }

    public LinoProtocolException(LinoProtocolErrorKind kind, string detail, Exception? innerException = null)
        : base($"{Describe(kind)}: {detail}", innerException)
    {
        Kind = kind;
        Detail = detail;
    }

    /// <summary>The category of the failure.</summary>
    public LinoProtocolErrorKind Kind { get; }

    /// <summary>The message without the category prefix.</summary>
    public string Detail { get; }

    internal static LinoProtocolException Malformed(string detail) => new(LinoProtocolErrorKind.Malformed, detail);

    internal static LinoProtocolException Limit(string detail) => new(LinoProtocolErrorKind.LimitExceeded, detail);

    internal static LinoProtocolException Unencodable(string detail) => new(LinoProtocolErrorKind.Unencodable, detail);

    private static string Describe(LinoProtocolErrorKind kind) => kind switch
    {
        LinoProtocolErrorKind.Io => "I/O error",
        LinoProtocolErrorKind.Malformed => "malformed message",
        LinoProtocolErrorKind.InvalidLino => "invalid LiNo",
        LinoProtocolErrorKind.LimitExceeded => "limit exceeded",
        LinoProtocolErrorKind.Unencodable => "cannot encode",
        LinoProtocolErrorKind.Remote => "server error",
        _ => kind.ToString(),
    };
}
