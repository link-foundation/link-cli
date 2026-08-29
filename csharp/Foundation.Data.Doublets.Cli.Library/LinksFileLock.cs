using System.Globalization;

namespace Foundation.Data.Doublets.Cli;

/// <summary>Requested sharing mode for a <see cref="LinksFileLock"/>.</summary>
public enum LockMode
{
    /// <summary>Multiple readers may hold the lock at the same time.</summary>
    Shared,
    /// <summary>Only one holder at a time; excludes all shared holders.</summary>
    Exclusive
}

/// <summary>
/// Advisory lock over a links database, for consumers that open the same
/// database file from more than one process.
/// </summary>
/// <remarks>
/// <para>
/// The lock is taken on a dedicated sidecar <c>*.lock</c> file rather than
/// on the database itself, so it survives operations that rewrite or remap
/// the database file.
/// </para>
/// <para>
/// Sharing is expressed through <see cref="FileShare"/>, which the runtime
/// maps onto the platform's own advisory locking (<c>flock</c> on Unix,
/// share modes on Windows). It is therefore honoured across processes, but
/// — like every advisory lock — only between participants that take it.
/// The operating system releases the lock when the holding process exits,
/// so a crashed writer never leaves a database permanently locked.
/// </para>
/// </remarks>
public sealed class LinksFileLock : IDisposable
{
    private readonly FileStream _stream;

    private LinksFileLock(FileStream stream, string path, LockMode mode)
    {
        _stream = stream;
        Path = path;
        Mode = mode;
    }

    /// <summary>The sidecar lock file this guard holds.</summary>
    public string Path { get; }

    /// <summary>The sharing mode this guard was acquired with.</summary>
    public LockMode Mode { get; }

    /// <summary>Conventional sidecar lock filename for a links database.</summary>
    public static string LockFilePath(string databaseFilename)
    {
        ArgumentNullException.ThrowIfNull(databaseFilename);
        return databaseFilename + ".lock";
    }

    /// <summary>
    /// Tries to acquire the lock, returning <see langword="null"/> when
    /// another holder currently owns a conflicting lock.
    /// </summary>
    public static LinksFileLock? TryAcquire(string lockPath, LockMode mode)
    {
        ArgumentNullException.ThrowIfNull(lockPath);
        EnsureDirectory(lockPath);
        var access = mode == LockMode.Exclusive ? FileAccess.ReadWrite : FileAccess.Read;
        var share = mode == LockMode.Exclusive ? FileShare.None : FileShare.Read;
        try
        {
            var stream = new FileStream(lockPath, FileMode.OpenOrCreate, access, share);
            return new LinksFileLock(stream, lockPath, mode);
        }
        catch (IOException)
        {
            return null;
        }
        catch (UnauthorizedAccessException)
        {
            // Windows reports a share-mode conflict on a locked file this way.
            return null;
        }
    }

    /// <summary>
    /// Acquires the lock, waiting until it becomes available or
    /// <paramref name="timeout"/> elapses.
    /// </summary>
    /// <exception cref="TimeoutException">The lock was still held when
    /// <paramref name="timeout"/> elapsed.</exception>
    public static LinksFileLock Acquire(string lockPath, LockMode mode, TimeSpan? timeout = null)
    {
        var deadline = timeout is null ? (DateTimeOffset?)null : DateTimeOffset.UtcNow + timeout.Value;
        var delay = TimeSpan.FromMilliseconds(1);
        while (true)
        {
            var acquired = TryAcquire(lockPath, mode);
            if (acquired is not null) return acquired;
            if (deadline is not null && DateTimeOffset.UtcNow >= deadline.Value)
            {
                throw new TimeoutException(
                  $"Timed out waiting for the {mode.ToString().ToLowerInvariant()} lock on '{lockPath}'.");
            }
            Thread.Sleep(delay);
            if (delay < TimeSpan.FromMilliseconds(50)) delay += delay;
        }
    }

    /// <summary>Releases the lock.</summary>
    public void Dispose() => _stream.Dispose();

    private static void EnsureDirectory(string lockPath)
    {
        var directory = System.IO.Path.GetDirectoryName(lockPath);
        if (!string.IsNullOrEmpty(directory) && !Directory.Exists(directory))
        {
            Directory.CreateDirectory(directory);
        }
    }
}

/// <summary>
/// Cheap fingerprint of a database file, used to answer "has anyone else
/// written since I last looked?" without reparsing the database.
/// </summary>
/// <param name="Length">Size of the database file in bytes (0 when it does
/// not exist).</param>
/// <param name="ModifiedTicks">Last write time in UTC ticks (0 when the
/// file does not exist).</param>
public readonly record struct StorageRevision(long Length, long ModifiedTicks)
{
    /// <summary>
    /// Reads the current revision of <paramref name="databaseFilename"/>.
    /// A missing file is reported as the default revision rather than an
    /// error, so a database can be fingerprinted before it is created.
    /// </summary>
    public static StorageRevision Of(string databaseFilename)
    {
        ArgumentNullException.ThrowIfNull(databaseFilename);
        var info = new FileInfo(databaseFilename);
        if (!info.Exists) return default;
        return new StorageRevision(info.Length, info.LastWriteTimeUtc.Ticks);
    }

    /// <summary>
    /// Whether <paramref name="databaseFilename"/> has changed since this
    /// revision was taken.
    /// </summary>
    public bool HasChanged(string databaseFilename) => Of(databaseFilename) != this;

    /// <inheritdoc/>
    public override string ToString()
    {
        return string.Create(
          CultureInfo.InvariantCulture, $"StorageRevision(Length={Length}, ModifiedTicks={ModifiedTicks})");
    }
}
