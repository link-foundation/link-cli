namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>
/// <c>clink</c> locks its stores for the whole run (issue #194): concurrent
/// <c>--in</c> imports all keep their writes, a held lock fails naming its
/// owner, and the lock options. The Rust counterpart is
/// <c>rust/tests/cli_store_lock_tests.rs</c>, the JavaScript one
/// <c>js/server/test/main-store-lock.test.mjs</c>.
/// </summary>
public sealed class CliStoreLockTests : IDisposable
{
    private readonly string _directory = Directory.CreateTempSubdirectory("cli-store-lock-").FullName;

    public void Dispose() => Directory.Delete(_directory, recursive: true);

    private string Db => Path.Combine(_directory, "s.links");

    [Theory]
    [InlineData(null)]
    [InlineData("flock")]
    [InlineData("pid")]
    public async Task ConcurrentImportsAllKeepTheirWrites(string? kind)
    {
        string[] lockArguments = kind is null ? [] : ["--lock", kind];
        const int Writers = 8;
        var imports = Enumerable.Range(1, Writers).Select(index =>
        {
            var input = Path.Combine(_directory, $"in{index}.lino");
            File.WriteAllText(input, $"(w{index}: w{index} w{index})\n");
            return Clink.RunAsync(["--db", Db, .. lockArguments, "--in", input]);
        });
        foreach (var result in await Task.WhenAll(imports)) Clink.AssertSucceeded(result);

        var output = Path.Combine(_directory, "out.lino");
        Clink.AssertSucceeded(await Clink.RunAsync("--db", Db, "--out", output));
        var exported = await File.ReadAllTextAsync(output);
        for (var index = 1; index <= Writers; index++)
        {
            Assert.Contains($"(w{index}: w{index} w{index})", exported, StringComparison.Ordinal);
        }
        Assert.DoesNotContain(
          Directory.GetFileSystemEntries(_directory),
          entry => entry.EndsWith(".owner", StringComparison.Ordinal) || entry.Contains(".lock.d", StringComparison.Ordinal));
    }

    [Fact]
    public async Task FailsNamingTheOwnerOfAHeldLockAndNoLockPassesItBy()
    {
        var directory = StorePidLock.LockDirectoryPath(Db);
        Directory.CreateDirectory(directory);
        File.WriteAllText(Path.Combine(directory, "owner"), StoreLockOwner.Current("held-by-test").ToRecord());

        var held = await Clink.RunAsync("--db", Db, "--lock", "pid", "--lock-timeout", "20", "() ((1 1))");
        Assert.Equal(1, held.ExitCode);
        Assert.StartsWith(
          $"Error: Lock error: store is locked: {directory} is held by pid={Environment.ProcessId} ",
          held.Stderr,
          StringComparison.Ordinal);
        Assert.Contains("gave up waiting for the pid lock after 20 ms (--lock-timeout)", held.Stderr, StringComparison.Ordinal);

        Clink.AssertSucceeded(await Clink.RunAsync("--db", Db, "--no-lock", "() ((1 1))"));
        var after = await Clink.RunAsync("--db", Db, "--lock", "none", "--after");
        Assert.Equal("(1: 1 1)\n", after.Stdout);
    }

    [Fact]
    public async Task AWriterWaitsForAReaderToFinish()
    {
        Clink.AssertSucceeded(await Clink.RunAsync("--db", Db, "() ((1 1))"));
        using var reader = StoreLock.Acquire([Db], LockMode.Shared, new StoreLockOptions { TimeoutMs = 0 });
        var blocked = await Clink.RunAsync("--db", Db, "--lock-timeout", "0", "() ((1 2))");
        Assert.Equal(1, blocked.ExitCode);
        Assert.Contains($"{Db}.lock is held by shared readers;", blocked.Stderr, StringComparison.Ordinal);
        var after = await Clink.RunAsync("--db", Db, "--lock-timeout", "0", "--after");
        if (OperatingSystem.IsWindows())
        {
            // Windows gives the store, mapped for writing, to one process, so
            // readers there take the lock alone too.
            Assert.Equal(1, after.ExitCode);
            Assert.Contains($"{Db}.lock is held by shared readers;", after.Stderr, StringComparison.Ordinal);
            return;
        }
        // Elsewhere readers share it.
        Clink.AssertSucceeded(after);
        Assert.Equal("(1: 1 1)\n", after.Stdout);
    }

    [Fact]
    public async Task ReadersOfAStoreNotCreatedYetTakeTheLockAlone()
    {
        // Opening a new store creates and initializes its memory-mapped files;
        // readers doing that at once crashed each other.
        var readers = Enumerable.Range(0, 8).Select(_ => Clink.RunAsync("--db", Db, "--after"));
        foreach (var result in await Task.WhenAll(readers)) Clink.AssertSucceeded(result);

        using var reader = StoreLock.Acquire([Path.Combine(_directory, "new.links")], LockMode.Shared, new StoreLockOptions { TimeoutMs = 0 });
        var first = await Clink.RunAsync("--db", Path.Combine(_directory, "new.links"), "--lock-timeout", "0", "--after");
        Assert.Equal(1, first.ExitCode);
        Assert.Contains("new.links.lock is held by shared readers;", first.Stderr, StringComparison.Ordinal);
    }

    [Fact]
    public async Task LockStatusReportsEveryStoreWithoutCreatingAny()
    {
        var log = Path.Combine(_directory, "log.links");
        var status = await Clink.RunAsync("--db", Db, "--transactions-file", log, "--lock-status");
        Clink.AssertSucceeded(status);
        Assert.Equal(
          $"flock {log}.lock: unlocked\npid {log}.lock.d: unlocked\nflock {Db}.lock: unlocked\npid {Db}.lock.d: unlocked\n",
          status.Stdout);
        Assert.Empty(Directory.GetFileSystemEntries(_directory));
    }

    [Theory]
    [InlineData("--lock", "mutex", "Invalid --lock value 'mutex'. Use 'flock', 'pid' or 'none'.")]
    [InlineData("--lock-timeout", "soon", "Invalid --lock-timeout value 'soon'. Use a whole number of milliseconds.")]
    public async Task ChecksTheLockOptionsBeforeAnyStoreOpens(string option, string value, string message)
    {
        var result = await Clink.RunAsync("--db", Db, option, value, "() ((1 1))");
        Assert.Equal(1, result.ExitCode);
        Assert.Equal(message + "\n", result.Stderr);
        Assert.Empty(Directory.GetFileSystemEntries(_directory));
    }

    [Fact]
    public void LocksTheDatabaseAndEverySideStoreOnce()
    {
        Assert.Equal(["a.links"], CliStoreLocks.LockedStores("a.links", null, null, null, " self ", " "));
        Assert.Equal(
          ["a.links", "t.links", "v.links", "g.links", "tokens.links", "p.links"],
          CliStoreLocks.LockedStores("a.links", "t.links", "v.links", "g.links", " tokens.links ", "p.links"));
    }
}
