using Link.Foundation.Links.Notation.Binary;

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
    /// <summary>The message exceeds one of the configured <see cref="ProtocolLimits"/>.</summary>
    LimitExceeded,
    /// <summary>The document cannot be represented with the chosen options.</summary>
    Unencodable,
    /// <summary>The server answered with an <c>(error: …)</c> document.</summary>
    Remote,
}

/// <summary>Raised by the LiNo network protocols (issue #105).</summary>
/// <remarks>
/// The codecs of <c>Link.Foundation.Links.Notation.Binary</c> raise a <see cref="BinaryNotationException"/>;
/// every protocol, server, client and archive entry point converts it into this exception
/// with the same kind and detail, keeping the original as <see cref="Exception.InnerException"/>.
/// </remarks>
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

    /// <summary>The same failure as <paramref name="error"/>, raised by the links-notation codecs.</summary>
    public static LinoProtocolException From(BinaryNotationException error)
    {
        ArgumentNullException.ThrowIfNull(error);
        var kind = error.Kind switch
        {
            BinaryErrorKind.Io => LinoProtocolErrorKind.Io,
            BinaryErrorKind.Malformed => LinoProtocolErrorKind.Malformed,
            BinaryErrorKind.InvalidLino => LinoProtocolErrorKind.InvalidLino,
            BinaryErrorKind.LimitExceeded => LinoProtocolErrorKind.LimitExceeded,
            BinaryErrorKind.Unencodable => LinoProtocolErrorKind.Unencodable,
            _ => LinoProtocolErrorKind.Malformed,
        };
        return new LinoProtocolException(kind, error.Detail, error);
    }

    /// <summary>Runs <paramref name="action"/>, converting a <see cref="BinaryNotationException"/> with <see cref="From"/>.</summary>
    internal static T Wrap<T>(Func<T> action)
    {
        try
        {
            return action();
        }
        catch (BinaryNotationException error)
        {
            throw From(error);
        }
    }

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
