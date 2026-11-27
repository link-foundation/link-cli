namespace Foundation.Data.Doublets.Cli;

/// <summary>
/// <c>--lock pid</c> (issue #194): a lock directory, <c>&lt;db&gt;.lock.d</c>,
/// holding an <c>owner</c> record.
/// </summary>
/// <remarks>
/// The directory is built under a private name with its record inside, then
/// renamed to <c>&lt;db&gt;.lock.d</c>. A rename onto an existing directory
/// fails, so exactly one process publishes the lock, and a published lock
/// always has its record. The operating system does not release it when the
/// owner exits, so a waiter breaks the lock when the record names a process
/// on this host, in this PID namespace, that is gone. Works where
/// <c>flock</c> does not, such as on some network file systems. The Rust and
/// JavaScript <c>clink</c> take the same lock.
/// </remarks>
public sealed class StorePidLock : IDisposable
{
    /// <summary>
    /// A directory without a readable record is treated as abandoned after
    /// this long (an owner crashed between creating and renaming it, or
    /// another tool made it).
    /// </summary>
    public static readonly TimeSpan UnownedGrace = TimeSpan.FromSeconds(10);

    private const string OwnerFile = "owner";

    private readonly string _token;
    private readonly bool _trace;

    private StorePidLock(string directory, string token, bool trace)
    {
        Path = directory;
        _token = token;
        _trace = trace;
    }

    /// <summary>The lock directory.</summary>
    public string Path { get; }

    /// <summary><c>&lt;db&gt;.lock.d</c>.</summary>
    public static string LockDirectoryPath(string store)
    {
        ArgumentNullException.ThrowIfNull(store);
        return store + ".lock.d";
    }

    /// <summary>The owner record of <paramref name="directory"/>, if it has a readable one.</summary>
    public static StoreLockOwner? ReadOwner(string directory)
    {
        try
        {
            using var file = new FileStream(System.IO.Path.Combine(directory, OwnerFile),
              FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            using var reader = new StreamReader(file);
            return StoreLockOwner.Parse(reader.ReadToEnd());
        }
        catch (IOException)
        {
            return null;
        }
        catch (UnauthorizedAccessException)
        {
            return null;
        }
    }

    /// <summary>Why the lock in <paramref name="directory"/> may be broken, if it may.</summary>
    public static string? StaleReason(string directory, StoreLockOwner? owner)
    {
        if (owner is not null)
        {
            var liveness = owner.Liveness();
            return liveness.State == StoreLockLivenessState.Gone ? liveness.Reason : null;
        }
        if (!Directory.Exists(directory)) return null;
        var age = DateTime.UtcNow - Directory.GetLastWriteTimeUtc(directory);
        return age >= UnownedGrace ? $"no owner record for {UnownedGrace.TotalSeconds} s" : null;
    }

    /// <summary>
    /// Publishes the lock once, breaking a stale one first. Returns the lock,
    /// or <see langword="null"/> and in <paramref name="holder"/> the owner of
    /// the lock that holds (<see langword="null"/> for a directory without a
    /// record).
    /// </summary>
    public static StorePidLock? TryAcquire(string directory, bool trace, out StoreLockOwner? holder)
    {
        ArgumentNullException.ThrowIfNull(directory);
        var parent = System.IO.Path.GetDirectoryName(directory);
        if (!string.IsNullOrEmpty(parent)) Directory.CreateDirectory(parent);
        var token = StoreLockOwner.UniqueToken();
        var owner = StoreLockOwner.Current(token);
        Exception? lastError = null;
        for (var attempt = 0; attempt < 4; attempt++)
        {
            var staging = $"{directory}.{owner.Pid}.{StoreLockOwner.UniqueToken()}.tmp";
            Directory.CreateDirectory(staging);
            try
            {
                WriteOwner(staging, owner);
                Directory.Move(staging, directory);
                holder = null;
                return new StorePidLock(directory, token, trace);
            }
            catch (IOException error)
            {
                lastError = error;
            }
            catch (UnauthorizedAccessException error)
            {
                lastError = error;
            }
            TryCleanup(() => Directory.Delete(staging, recursive: true), trace);
            if (!Exists(directory))
            {
                // Released between the rename and this check, or a real error.
                continue;
            }
            lastError = null;
            holder = ReadOwner(directory);
            var reason = StaleReason(directory, holder);
            if (reason is null) return null;
            if (trace) Console.Error.WriteLine($"[TRACE] Breaking stale lock {directory}: {reason}");
            BreakStale(directory, holder, trace);
        }
        if (lastError is not null) throw new IOException($"Cannot create the lock {directory}: {lastError.Message}", lastError);
        holder = ReadOwner(directory);
        return null;
    }

    /// <summary>
    /// Moves a stale lock aside and removes it. Should a live owner have
    /// published a new lock in between, its directory is moved back. A lock
    /// that cannot be moved (another waiter moved it first, or the directory
    /// is not writable) is left alone: the next attempt sees what became of it.
    /// </summary>
    public static void BreakStale(string directory, StoreLockOwner? expected) => BreakStale(directory, expected, false);

    private static void BreakStale(string directory, StoreLockOwner? expected, bool trace)
    {
        ArgumentNullException.ThrowIfNull(directory);
        var tombstone = $"{directory}.{Environment.ProcessId}.{StoreLockOwner.UniqueToken()}.stale";
        // Let the next acquisition reread the owner before another attempt.
        if (!TryCleanup(() => Directory.Move(directory, tombstone), trace, retry: false)) return;
        var moved = ReadOwner(tombstone);
        if (moved?.Token == expected?.Token)
        {
            TryCleanup(() => Directory.Delete(tombstone, recursive: true), trace);
        }
        else
        {
            // A third process may have published meanwhile; then the moved
            // owner has lost its lock and its directory stays for inspection.
            TryCleanup(() => Directory.Move(tombstone, directory), trace, retry: false);
        }
    }

    /// <summary>Removes the lock while it is still ours.</summary>
    public void Dispose()
    {
        if (ReadOwner(Path)?.Token != _token) return;
        var tombstone = $"{Path}.{_token}.released";
        if (TryCleanup(() => Directory.Move(Path, tombstone), _trace))
            TryCleanup(() => Directory.Delete(tombstone, recursive: true), _trace);
    }

    private static void WriteOwner(string directory, StoreLockOwner owner)
    {
        using var file = new FileStream(System.IO.Path.Combine(directory, OwnerFile), FileMode.CreateNew, FileAccess.Write);
        var bytes = System.Text.Encoding.UTF8.GetBytes(owner.ToRecord());
        file.Write(bytes);
        file.Flush(flushToDisk: true);
    }

    /// <summary>Whether anything, even a dangling link, is at <paramref name="path"/>.</summary>
    internal static bool Exists(string path) => Directory.Exists(path) || File.Exists(path) || new FileInfo(path).LinkTarget is not null;

    internal static bool TryCleanup(Action cleanup, bool trace, bool retry = true)
    {
        // On Windows a waiter can still have the owner record open when the
        // holder renames the containing directory. Retry for at most 75 ms.
        for (var delay = 5; ; delay *= 2)
        {
            try
            {
                cleanup();
                return true;
            }
            catch (Exception error) when (error is IOException or UnauthorizedAccessException)
            {
                const int accessDenied = unchecked((int)0x80070005);
                const int sharingViolation = unchecked((int)0x80070020);
                const int lockViolation = unchecked((int)0x80070021);
                if (trace) Console.Error.WriteLine($"[TRACE] PID-lock cleanup: 0x{error.HResult:X8} {error.Message}");
                if (!retry || !OperatingSystem.IsWindows() || delay > 40 ||
                    error.HResult is not (accessDenied or sharingViolation or lockViolation))
                    return false;
                Thread.Sleep(delay);
            }
        }
    }
}
