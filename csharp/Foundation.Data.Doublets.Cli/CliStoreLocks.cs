using System.CommandLine;

namespace Foundation.Data.Doublets.Cli;

/// <summary>
/// <c>--lock</c>, <c>--no-lock</c>, <c>--lock-timeout</c> and
/// <c>--lock-status</c>: the store locks every invocation takes (issue #194),
/// as the Rust and JavaScript <c>clink</c> take them. See <see cref="StoreLock"/>.
/// </summary>
public sealed class CliStoreLocks
{
    private readonly Option<string?> _kind = new("--lock")
    {
        Description = "How concurrent invocations on one store are serialized: 'flock' (default; an OS lock on <db>.lock, "
            + "released when the holder exits), 'pid' (the directory <db>.lock.d, broken once its owner process is gone) "
            + "or 'none'. Writers lock exclusively, read-only runs share it",
    };

    private readonly Option<bool> _none = new("--no-lock")
    {
        Description = "Same as --lock none, for callers that serialize access themselves",
        DefaultValueFactory = _ => false,
    };

    private readonly Option<string?> _timeout = new("--lock-timeout")
    {
        Description = "How long to wait for a locked store before failing with the name of its owner, "
            + "in milliseconds (default: 30000; 0 fails at once)",
    };

    private readonly Option<bool> _status = new("--lock-status")
    {
        Description = "Print who holds the locks of the store, then exit",
        DefaultValueFactory = _ => false,
    };

    public void AddTo(RootCommand command)
    {
        ArgumentNullException.ThrowIfNull(command);
        command.Options.Add(_kind);
        command.Options.Add(_none);
        command.Options.Add(_timeout);
        command.Options.Add(_status);
    }

    /// <summary>
    /// The database and every side store given: the transitions log, the
    /// version-control store, the triggers store, and the token and permission
    /// stores unless they are kept in the database itself (<c>self</c>).
    /// </summary>
    public static IReadOnlyList<string> LockedStores(
      string db, string? transactionsFile, string? vcFile, string? triggersFile, string? tokens, string? permissions)
    {
        var stores = new List<string> { db };
        foreach (var side in new[] { transactionsFile, vcFile, triggersFile })
        {
            if (side is not null) stores.Add(side);
        }
        foreach (var source in new[] { tokens, permissions })
        {
            var trimmed = source?.Trim();
            if (!string.IsNullOrEmpty(trimmed) && trimmed != "self") stores.Add(trimmed);
        }
        return stores;
    }

    /// <summary>
    /// Locks <paramref name="stores"/>, shared for a read-only run and
    /// exclusively otherwise. Returns the lock to hold until the stores are
    /// saved, or <see langword="null"/> with the status to exit with in
    /// <paramref name="exitCode"/>: after <c>--lock-status</c>, an invalid
    /// option, or a store that stayed locked.
    /// </summary>
    public StoreLock? Acquire(ParseResult parsed, IReadOnlyList<string> stores, bool readOnly, bool trace, out int? exitCode)
    {
        ArgumentNullException.ThrowIfNull(parsed);
        StoreLockOptions options;
        try
        {
            var kind = parsed.GetValue(_none) ? "none" : parsed.GetValue(_kind);
            options = StoreLockOptions.Parse(kind, parsed.GetValue(_timeout)) with { Trace = trace };
        }
        catch (FormatException error)
        {
            Console.Error.WriteLine(error.Message);
            exitCode = 1;
            return null;
        }
        var ordered = StoreLock.LockOrder(stores);
        if (parsed.GetValue(_status))
        {
            Console.Write(StoreLock.Status(ordered));
            exitCode = 0;
            return null;
        }
        try
        {
            exitCode = null;
            return StoreLock.Acquire(ordered, readOnly ? LockMode.Shared : LockMode.Exclusive, options);
        }
        catch (StoreLockedException error)
        {
            Console.Error.WriteLine($"Error: Lock error: {error.Message}");
            exitCode = 1;
            return null;
        }
    }
}
