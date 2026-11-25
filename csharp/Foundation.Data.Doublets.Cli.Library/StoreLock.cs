using System.Diagnostics;
using System.Globalization;
using System.Text;

namespace Foundation.Data.Doublets.Cli;

/// <summary>The <c>--lock</c> mechanism.</summary>
public enum StoreLockKind
{
    /// <summary>An operating system lock on <c>&lt;db&gt;.lock</c> (<see cref="StoreFileLock"/>).</summary>
    Flock,
    /// <summary>The lock directory <c>&lt;db&gt;.lock.d</c> (<see cref="StorePidLock"/>).</summary>
    Pid,
    /// <summary>No lock: the caller serializes access itself.</summary>
    None,
}

/// <summary>A store stayed locked until <c>--lock-timeout</c> passed.</summary>
public sealed class StoreLockedException : Exception
{
    public StoreLockedException() { }

    public StoreLockedException(string message) : base(message) { }

    public StoreLockedException(string message, Exception innerException) : base(message, innerException) { }
}

/// <summary>The parsed <c>--lock</c>, <c>--no-lock</c> and <c>--lock-timeout</c> options.</summary>
public sealed record StoreLockOptions
{
    /// <summary>How long a waiter waits by default, in milliseconds.</summary>
    public const ulong DefaultTimeoutMs = 30_000;

    public StoreLockKind Kind { get; init; } = StoreLockKind.Flock;

    /// <summary>How long to wait for a held lock, in milliseconds; zero tries once.</summary>
    public ulong TimeoutMs { get; init; } = DefaultTimeoutMs;

    /// <summary>Report waiting, breaking and acquiring on standard error.</summary>
    public bool Trace { get; init; }

    /// <summary>Reads the raw values of <c>--lock</c> and <c>--lock-timeout</c>.</summary>
    /// <exception cref="FormatException">A value is not one of the accepted ones.</exception>
    public static StoreLockOptions Parse(string? kind, string? timeout)
    {
        var options = new StoreLockOptions();
        var name = kind?.Trim();
        if (!string.IsNullOrEmpty(name))
        {
            options = options with
            {
                Kind = name.ToLowerInvariant() switch
                {
                    "flock" => StoreLockKind.Flock,
                    "pid" => StoreLockKind.Pid,
                    "none" => StoreLockKind.None,
                    _ => throw new FormatException($"Invalid --lock value '{name}'. Use 'flock', 'pid' or 'none'."),
                }
            };
        }
        if (timeout is not null)
        {
            var value = timeout.Trim();
            if (!ulong.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out var milliseconds))
            {
                throw new FormatException($"Invalid --lock-timeout value '{value}'. Use a whole number of milliseconds.");
            }
            options = options with { TimeoutMs = milliseconds };
        }
        return options;
    }
}

/// <summary>
/// Store locking for <c>clink</c> invocations (issue #194).
/// </summary>
/// <remarks>
/// <para>
/// Every invocation reads the store, changes it and saves it back. Two
/// invocations that overlap must not interleave those steps, or the later
/// one acts on what the earlier one is about to change. A
/// <see cref="StoreLock"/> makes the whole run exclusive for writers and
/// shared among readers; a waiter gives up with a "store is locked" error
/// naming the owner once <c>--lock-timeout</c> passes.
/// </para>
/// <para>
/// <c>--lock flock</c> (the default) takes <see cref="StoreFileLock"/> on
/// <c>&lt;db&gt;.lock</c>; <c>--lock pid</c> the directory
/// <c>&lt;db&gt;.lock.d</c> (<see cref="StorePidLock"/>), always exclusively;
/// <c>--lock none</c> nothing. An exclusive <c>flock</c> holder writes its
/// owner record to <c>&lt;db&gt;.lock.owner</c> and removes it on release, so
/// that waiters and <c>--lock-status</c> can name it. The Rust and JavaScript
/// <c>clink</c> take the same locks.
/// </para>
/// </remarks>
public sealed class StoreLock : IDisposable
{
    private readonly List<IDisposable> _guards;

    private StoreLock(List<IDisposable> guards) => _guards = guards;

    /// <summary>The lock files and directories held.</summary>
    public IReadOnlyList<string> Paths => _guards
      .Select(guard => guard switch
      {
          FlockGuard flock => flock.Lock.Path,
          StorePidLock pid => pid.Path,
          _ => throw new UnreachableException(),
      })
      .ToList();

    /// <summary><c>&lt;db&gt;.lock.owner</c>: the record of the exclusive <c>flock</c> holder.</summary>
    public static string OwnerFilePath(string store) => LinksFileLock.LockFilePath(store) + ".owner";

    /// <summary>
    /// The stores of one invocation without duplicates, in the order every
    /// process locks them: sorted by absolute path. Each keeps the spelling it
    /// was given, which is what messages show.
    /// </summary>
    public static IReadOnlyList<string> LockOrder(IEnumerable<string> stores)
    {
        ArgumentNullException.ThrowIfNull(stores);
        var ordered = new List<(string Key, string Store)>();
        foreach (var (key, store) in stores.Select(store => (Normalize(store), store)).OrderBy(pair => pair.Item1, StringComparer.Ordinal))
        {
            if (ordered.Count == 0 || !SamePath(ordered[^1].Key, key)) ordered.Add((key, store));
        }
        return ordered.Select(pair => pair.Store).ToList();
    }

    /// <summary>
    /// Locks every store in <paramref name="stores"/> (see
    /// <see cref="LockOrder"/>), waiting up to <c>--lock-timeout</c> for each.
    /// </summary>
    /// <exception cref="StoreLockedException">A store stayed locked.</exception>
    public static StoreLock Acquire(IReadOnlyList<string> stores, LockMode mode, StoreLockOptions options)
    {
        ArgumentNullException.ThrowIfNull(stores);
        ArgumentNullException.ThrowIfNull(options);
        var started = Stopwatch.StartNew();
        var guards = new List<IDisposable>();
        try
        {
            foreach (var store in stores)
            {
                switch (options.Kind)
                {
                    case StoreLockKind.Flock:
                        guards.Add(AcquireFlock(store, mode, started, options));
                        break;
                    case StoreLockKind.Pid:
                        guards.Add(AcquirePid(store, started, options));
                        break;
                }
            }
            var acquired = new StoreLock(guards);
            guards = [];
            return acquired;
        }
        finally
        {
            Release(guards);
        }
    }

    /// <summary>Releases the locks, the last taken first.</summary>
    public void Dispose()
    {
        Release(_guards);
        _guards.Clear();
    }

    private static void Release(List<IDisposable> guards)
    {
        for (var index = guards.Count - 1; index >= 0; index--) guards[index].Dispose();
    }

    /// <summary>
    /// <c>--lock-status</c>: one line per store and lock kind, without taking
    /// or creating any lock that is not already there.
    /// </summary>
    public static string Status(IReadOnlyList<string> stores)
    {
        ArgumentNullException.ThrowIfNull(stores);
        var report = new StringBuilder();
        foreach (var store in stores)
        {
            var path = LinksFileLock.LockFilePath(store);
            string state;
            if (!File.Exists(path))
            {
                state = "unlocked";
            }
            else
            {
                try
                {
                    using var probe = StoreFileLock.TryAcquire(path, LockMode.Exclusive);
                    state = probe is not null ? "unlocked" : $"locked by {FlockHolder(store)}";
                }
                catch (IOException error)
                {
                    state = $"unknown ({error.Message})";
                }
                catch (UnauthorizedAccessException error)
                {
                    state = $"unknown ({error.Message})";
                }
            }
            report.Append(CultureInfo.InvariantCulture, $"flock {path}: {state}\n");
            var directory = StorePidLock.LockDirectoryPath(store);
            if (!StorePidLock.Exists(directory))
            {
                state = "unlocked";
            }
            else
            {
                var owner = StorePidLock.ReadOwner(directory);
                state = $"locked by {DescribePidHolder(directory, owner)}";
                if (StorePidLock.StaleReason(directory, owner) is { } reason)
                {
                    state += $"; stale, the next writer breaks it: {reason}";
                }
            }
            report.Append(CultureInfo.InvariantCulture, $"pid {directory}: {state}\n");
        }
        return report.ToString();
    }

    /// <summary>An exclusive or shared <c>flock</c>; an exclusive holder keeps its owner record.</summary>
    private sealed class FlockGuard(StoreFileLock fileLock, string owner) : IDisposable
    {
        public StoreFileLock Lock { get; } = fileLock;

        public void Dispose()
        {
            // Before the lock itself is released.
            if (Lock.Mode == LockMode.Exclusive) TryDeleteFile(owner);
            Lock.Dispose();
        }
    }

    private static FlockGuard AcquireFlock(string store, LockMode mode, Stopwatch started, StoreLockOptions options)
    {
        var path = LinksFileLock.LockFilePath(store);
        var kind = mode == LockMode.Exclusive ? "exclusive flock" : "shared flock";
        var backoff = new Backoff(started, options.TimeoutMs);
        var reported = false;
        while (true)
        {
            var fileLock = StoreFileLock.TryAcquire(path, mode);
            if (fileLock is not null)
            {
                try
                {
                    var owner = OwnerFilePath(store);
                    if (mode == LockMode.Exclusive)
                    {
                        WriteRecord(owner, StoreLockOwner.Current(StoreLockOwner.UniqueToken()).ToRecord());
                    }
                    else
                    {
                        // No exclusive holder exists, so a record is a crashed one's.
                        TryDeleteFile(owner);
                    }
                    if (options.Trace) Console.Error.WriteLine($"[TRACE] Acquired {kind} lock {path}");
                    var guard = new FlockGuard(fileLock, owner);
                    fileLock = null;
                    return guard;
                }
                finally
                {
                    fileLock?.Dispose();
                }
            }
            if (options.Trace && !reported)
            {
                reported = true;
                Console.Error.WriteLine($"[TRACE] Waiting for {kind} lock {path}: held by {FlockHolder(store)}");
            }
            if (!backoff.Wait()) throw LockedError(path, kind, FlockHolder(store), options);
        }
    }

    /// <summary>Who holds the <c>&lt;db&gt;.lock</c> of <paramref name="store"/>: shared readers, or the writer its record names.</summary>
    private static string FlockHolder(string store)
    {
        using (var probe = StoreFileLock.TryAcquire(LinksFileLock.LockFilePath(store), LockMode.Shared))
        {
            if (probe is not null) return "shared readers";
        }
        string? text;
        try
        {
            text = File.ReadAllText(OwnerFilePath(store));
        }
        catch (IOException)
        {
            text = null;
        }
        catch (UnauthorizedAccessException)
        {
            text = null;
        }
        return StoreLockOwner.Parse(text) is { } owner
          ? $"an exclusive writer: {owner.Describe()}"
          : "an exclusive writer that left no owner record";
    }

    private static StorePidLock AcquirePid(string store, Stopwatch started, StoreLockOptions options)
    {
        var directory = StorePidLock.LockDirectoryPath(store);
        var backoff = new Backoff(started, options.TimeoutMs);
        var reported = false;
        while (true)
        {
            var acquired = StorePidLock.TryAcquire(directory, options.Trace, out var owner);
            if (acquired is not null)
            {
                if (options.Trace) Console.Error.WriteLine($"[TRACE] Acquired pid lock {directory}");
                return acquired;
            }
            var holder = DescribePidHolder(directory, owner);
            if (options.Trace && !reported)
            {
                reported = true;
                Console.Error.WriteLine($"[TRACE] Waiting for pid lock {directory}: held by {holder}");
            }
            if (!backoff.Wait()) throw LockedError(directory, "pid", holder, options);
        }
    }

    private static string DescribePidHolder(string directory, StoreLockOwner? owner) =>
        owner?.Describe()
          ?? $"a directory without an owner record (removed once {directory} is {StorePidLock.UnownedGrace.TotalSeconds} s old)";

    private static StoreLockedException LockedError(string path, string kind, string holder, StoreLockOptions options) =>
        new(string.Create(
          CultureInfo.InvariantCulture,
          $"store is locked: {path} is held by {holder}; gave up waiting for the {kind} lock after {options.TimeoutMs} ms (--lock-timeout)"));

    /// <summary>
    /// Replaces the record at <paramref name="path"/> at once, so that no
    /// reader sees half of it. Best effort: the record only serves diagnostics.
    /// </summary>
    internal static void WriteRecord(string path, string record)
    {
        var staging = $"{path}.{StoreLockOwner.UniqueToken()}.tmp";
        try
        {
            File.WriteAllText(staging, record);
            File.Move(staging, path, overwrite: true);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            TryDeleteFile(staging);
        }
    }

    private static void TryDeleteFile(string path)
    {
        try
        {
            File.Delete(path);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            // Best effort, as the record it removes.
        }
    }

    private static string Normalize(string store)
    {
        var absolute = System.IO.Path.GetFullPath(store);
        var parent = System.IO.Path.GetDirectoryName(absolute);
        var name = System.IO.Path.GetFileName(absolute);
        if (string.IsNullOrEmpty(parent) || string.IsNullOrEmpty(name)) return absolute;
        try
        {
            var resolved = new DirectoryInfo(parent).ResolveLinkTarget(returnFinalTarget: true)?.FullName ?? parent;
            return System.IO.Path.Join(resolved, name);
        }
        catch (IOException)
        {
            return absolute;
        }
    }

    private static bool SamePath(string left, string right) =>
        string.Equals(left, right, OperatingSystem.IsWindows() ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal);

    /// <summary>Waits from 1 ms, doubling up to 50 ms, until the timeout passes.</summary>
    private sealed class Backoff(Stopwatch started, ulong timeoutMs)
    {
        private int _delayMs = 1;

        /// <summary>Sleeps before the next attempt; false once the timeout has passed.</summary>
        public bool Wait()
        {
            var elapsed = (ulong)started.ElapsedMilliseconds;
            if (elapsed >= timeoutMs) return false;
            var left = timeoutMs - elapsed;
            Thread.Sleep((int)Math.Min((ulong)_delayMs, left));
            _delayMs = Math.Min(_delayMs * 2, 50);
            return true;
        }
    }
}
