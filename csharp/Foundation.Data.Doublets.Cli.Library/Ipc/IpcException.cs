namespace Foundation.Data.Doublets.Cli.Ipc;

/// <summary>What went wrong in an IPC transport, as the kinds of a Rust <c>io::Error</c>.</summary>
public enum IpcErrorKind
{
    /// <summary>Any other failure, usually of the operating system.</summary>
    Other,
    /// <summary>An endpoint or argument is not valid.</summary>
    InvalidInput,
    /// <summary>A handshake file or a shared region holds data that is not valid.</summary>
    InvalidData,
    /// <summary>A named object does not exist.</summary>
    NotFound,
    /// <summary>The operating system denied access.</summary>
    PermissionDenied,
    /// <summary>A named object already exists.</summary>
    AlreadyExists,
    /// <summary>Another live server already listens on the address.</summary>
    AddrInUse,
    /// <summary>Nobody listens on the address.</summary>
    ConnectionRefused,
    /// <summary>The peer hung up.</summary>
    ConnectionReset,
    /// <summary>The peer no longer reads.</summary>
    BrokenPipe,
    /// <summary>A read or a handshake did not finish in time.</summary>
    TimedOut,
    /// <summary>The kind or operation is not available on this platform.</summary>
    Unsupported,
}

/// <summary>A failure of an IPC transport (issue #140); the message matches the Rust implementation.</summary>
public sealed class IpcException : IOException
{
    /// <summary>Creates an error of <paramref name="kind"/>.</summary>
    public IpcException(IpcErrorKind kind, string message, Exception? inner = null) : base(message, inner)
    {
        Kind = kind;
    }

    /// <summary>Creates an <see cref="IpcErrorKind.Other"/> error.</summary>
    public IpcException() : this(IpcErrorKind.Other, "IPC transport failure") { }

    /// <summary>Creates an <see cref="IpcErrorKind.Other"/> error.</summary>
    public IpcException(string message) : this(IpcErrorKind.Other, message) { }

    /// <summary>Creates an <see cref="IpcErrorKind.Other"/> error.</summary>
    public IpcException(string message, Exception inner) : this(IpcErrorKind.Other, message, inner) { }

    /// <summary>What went wrong.</summary>
    public IpcErrorKind Kind { get; }

    internal static IpcException HungUp() => new(IpcErrorKind.ConnectionReset, "the peer hung up");

    internal static IpcException BrokenPipe() => new(IpcErrorKind.BrokenPipe, "the peer no longer reads");

    internal static IpcException TimedOut() => new(IpcErrorKind.TimedOut, "timed out");

    internal static IpcException Invalid(string message) => new(IpcErrorKind.InvalidInput, message);

    internal static IpcException InvalidData(string message) => new(IpcErrorKind.InvalidData, message);

    internal static IpcException Unsupported(string kind, string what) =>
        new(IpcErrorKind.Unsupported, $"{what} on `{kind}:` endpoints is not supported on this platform");

    /// <summary>Whether <paramref name="error"/> or one of its inner errors is a timeout.</summary>
    public static bool IsTimeout(Exception? error)
    {
        for (; error is not null; error = error.InnerException)
        {
            if (error is IpcException { Kind: IpcErrorKind.TimedOut } or TimeoutException)
            {
                return true;
            }
        }
        return false;
    }
}
