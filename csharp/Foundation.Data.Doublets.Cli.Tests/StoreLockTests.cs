namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>
/// The store locks of issue #194 (<see cref="StoreLock"/>): options, owner
/// records, the <c>flock</c> and <c>pid</c> locks, their order and
/// <c>--lock-status</c>. The Rust counterpart is <c>rust/src/store_lock</c>,
/// the JavaScript one <c>js/server/test/store-lock.test.mjs</c>.
/// </summary>
public sealed class StoreLockTests : IDisposable
{
    // No process has this id: Linux caps them at 2^22, Windows ids are multiples of 4.
    private const uint GonePid = 0x7FFF_FFF1;

    private readonly string _directory = Directory.CreateTempSubdirectory("store-lock-").FullName;

    public void Dispose() => Directory.Delete(_directory, recursive: true);

    private string Store(string name = "s.links") => Path.Combine(_directory, name);

    private static StoreLockOptions Immediately(StoreLockKind kind) => new() { Kind = kind, TimeoutMs = 0 };

    [Fact]
    public void OptionsDefaultToAFlockWaitingThirtySeconds()
    {
        var options = StoreLockOptions.Parse(null, null);
        Assert.Equal(StoreLockKind.Flock, options.Kind);
        Assert.Equal(30_000UL, options.TimeoutMs);
        Assert.Equal(StoreLockKind.Pid, StoreLockOptions.Parse(" PID ", " 0 ").Kind);
        Assert.Equal(0UL, StoreLockOptions.Parse("none", "0").TimeoutMs);
        Assert.Equal(StoreLockKind.Flock, StoreLockOptions.Parse("", null).Kind);
    }

    [Theory]
    [InlineData("mutex", null, "Invalid --lock value 'mutex'. Use 'flock', 'pid' or 'none'.")]
    [InlineData(null, "soon", "Invalid --lock-timeout value 'soon'. Use a whole number of milliseconds.")]
    [InlineData(null, "-1", "Invalid --lock-timeout value '-1'. Use a whole number of milliseconds.")]
    public void RejectsUnknownOptionValues(string? kind, string? timeout, string message)
    {
        Assert.Equal(message, Assert.Throws<FormatException>(() => StoreLockOptions.Parse(kind, timeout)).Message);
    }

    [Fact]
    public void OwnerRecordsRoundTripAndIgnoreUnknownKeys()
    {
        var owner = StoreLockOwner.Current("t-1");
        Assert.Equal((uint)Environment.ProcessId, owner.Pid);
        Assert.StartsWith("clink (csharp)", owner.Program, StringComparison.Ordinal);
        Assert.Equal(owner, StoreLockOwner.Parse(owner.ToRecord() + "added_later=1\nno separator\n"));
        Assert.Null(StoreLockOwner.Parse(null));
        Assert.Null(StoreLockOwner.Parse("host=h\n"));
        Assert.Null(StoreLockOwner.Parse("pid=x\n"));
        Assert.Equal(0UL, StoreLockOwner.Parse("pid=7\nacquired_at_ms=later\n")!.AcquiredAtMs);
    }

    [Fact]
    public void DescribesWhoHoldsALockAndWhetherItRuns()
    {
        var owner = StoreLockOwner.Current("t") with { AcquiredAtMs = 5, StartTime = "", PidNamespace = StoreLockOwner.CurrentPidNamespace() };
        Assert.Equal(StoreLockLivenessState.Alive, owner.Liveness().State);
        Assert.Matches($"^pid={Environment.ProcessId} host=\\S* .*acquired_at_ms=5 \\(running\\)$", owner.Describe());
        var gone = owner with { Pid = GonePid };
        Assert.Equal(new StoreLockLiveness(StoreLockLivenessState.Gone, $"owner process {GonePid} is gone"), gone.Liveness());
        Assert.EndsWith($"(owner process {GonePid} is gone)", gone.Describe(), StringComparison.Ordinal);
        var elsewhere = owner with { Host = "another-host" };
        Assert.Equal(StoreLockLivenessState.Unknown, elsewhere.Liveness(_ => false).State);
        Assert.Equal("held on another host, another-host", elsewhere.Liveness().Reason);
        var contained = owner with { PidNamespace = "pid:[1]" };
        Assert.Equal(new StoreLockLiveness(StoreLockLivenessState.Unknown, "held in another PID namespace"), contained.Liveness(_ => false));
    }

    [Fact]
    public void TellsAReusedProcessIdByItsStartTime()
    {
        var owner = StoreLockOwner.Current("t");
        var reused = owner with { StartTime = "1" };
        var expected = OperatingSystem.IsLinux()
          ? new StoreLockLiveness(StoreLockLivenessState.Gone, $"owner process {owner.Pid} is gone; its id now belongs to another process")
          : new StoreLockLiveness(StoreLockLivenessState.Alive, "");
        Assert.Equal(expected, reused.Liveness());
        Assert.Equal("77", StoreLockOwner.StartTimeOfStat("12 (a b) c) S " + string.Join(' ', Enumerable.Range(4, 18)) + " 77 1 2"));
        Assert.Null(StoreLockOwner.StartTimeOfStat("12 (short) S 1 2"));
        Assert.Null(StoreLockOwner.StartTimeOfStat("no command"));
        Assert.Null(StoreLockOwner.ProcessStartTime(GonePid));
    }

    [Fact]
    public void TokensDiffer()
    {
        Assert.NotEqual(StoreLockOwner.UniqueToken(), StoreLockOwner.UniqueToken());
        Assert.Matches($"^{Environment.ProcessId}\\.[0-9a-f]{{16}}$", StoreLockOwner.UniqueToken());
    }

    [Fact]
    public void LocksEveryStoreOnceInPathOrder()
    {
        var b = Store("b.links");
        var a = Store("a.links");
        var relativeA = Path.GetRelativePath(Environment.CurrentDirectory, a);
        Assert.Equal([a, b], StoreLock.LockOrder([b, a, relativeA, Path.Combine(_directory, ".", "b.links")]));
        Assert.Empty(StoreLock.LockOrder([]));
    }

    [Fact]
    public void AnExclusiveFlockKeepsItsOwnerRecordWhileHeld()
    {
        var store = Store();
        using (var held = StoreLock.Acquire([store], LockMode.Exclusive, Immediately(StoreLockKind.Flock)))
        {
            Assert.Equal([store + ".lock"], held.Paths);
            var owner = StoreLockOwner.Parse(File.ReadAllText(StoreLock.OwnerFilePath(store)));
            Assert.Equal((uint)Environment.ProcessId, owner!.Pid);
            var error = Assert.Throws<StoreLockedException>(
              () => StoreLock.Acquire([store], LockMode.Shared, Immediately(StoreLockKind.Flock)));
            Assert.StartsWith(
              $"store is locked: {store}.lock is held by an exclusive writer: pid={Environment.ProcessId} ",
              error.Message,
              StringComparison.Ordinal);
            Assert.EndsWith("; gave up waiting for the shared flock lock after 0 ms (--lock-timeout)", error.Message, StringComparison.Ordinal);
            Assert.Contains($"locked by an exclusive writer: pid={Environment.ProcessId}", StoreLock.Status([store]), StringComparison.Ordinal);
        }
        Assert.False(File.Exists(StoreLock.OwnerFilePath(store)));
        Assert.Equal(
          $"flock {store}.lock: unlocked\npid {store}.lock.d: unlocked\n",
          StoreLock.Status([store]));
    }

    [Fact]
    public void ReadersShareTheFlockAndExcludeWriters()
    {
        var store = Store();
        File.WriteAllText(StoreLock.OwnerFilePath(store), "pid=1\n");
        using var first = StoreLock.Acquire([store], LockMode.Shared, Immediately(StoreLockKind.Flock));
        // A record no exclusive holder keeps is a crashed one's.
        Assert.False(File.Exists(StoreLock.OwnerFilePath(store)));
        using var second = StoreLock.Acquire([store], LockMode.Shared, Immediately(StoreLockKind.Flock));
        var error = Assert.Throws<StoreLockedException>(
          () => StoreLock.Acquire([store], LockMode.Exclusive, new StoreLockOptions { TimeoutMs = 30 }));
        Assert.Equal(
          $"store is locked: {store}.lock is held by shared readers; gave up waiting for the exclusive flock lock after 30 ms (--lock-timeout)",
          error.Message);
        Assert.Equal($"flock {store}.lock: locked by shared readers\npid {store}.lock.d: unlocked\n", StoreLock.Status([store]));
    }

    [Fact]
    public void NamesAWriterThatLeftNoRecord()
    {
        var store = Store();
        using var held = StoreFileLock.TryAcquire(store + ".lock", LockMode.Exclusive);
        Assert.NotNull(held);
        var error = Assert.Throws<StoreLockedException>(
          () => StoreLock.Acquire([store], LockMode.Exclusive, Immediately(StoreLockKind.Flock)));
        Assert.Contains("is held by an exclusive writer that left no owner record;", error.Message, StringComparison.Ordinal);
    }

    [Fact]
    public void ReleasesWhatItTookWhenALaterStoreStaysLocked()
    {
        var first = Store("a.links");
        var second = Store("b.links");
        using (StoreFileLock.TryAcquire(second + ".lock", LockMode.Exclusive))
        {
            Assert.Throws<StoreLockedException>(
              () => StoreLock.Acquire([first, second], LockMode.Exclusive, Immediately(StoreLockKind.Flock)));
        }
        using var again = StoreFileLock.TryAcquire(first + ".lock", LockMode.Exclusive);
        Assert.NotNull(again);
    }

    [Fact]
    public void NoneTakesNothing()
    {
        var store = Store();
        using var held = StoreLock.Acquire([store], LockMode.Exclusive, Immediately(StoreLockKind.None));
        Assert.Empty(held.Paths);
        Assert.Empty(Directory.GetFileSystemEntries(_directory));
    }

    [Fact]
    public void APidLockExcludesOthersUntilReleased()
    {
        var store = Store();
        var directory = StorePidLock.LockDirectoryPath(store);
        using (var held = StoreLock.Acquire([store], LockMode.Shared, Immediately(StoreLockKind.Pid)))
        {
            Assert.Equal([directory], held.Paths);
            Assert.Equal((uint)Environment.ProcessId, StorePidLock.ReadOwner(directory)!.Pid);
            var error = Assert.Throws<StoreLockedException>(
              () => StoreLock.Acquire([store], LockMode.Shared, Immediately(StoreLockKind.Pid)));
            Assert.StartsWith($"store is locked: {directory} is held by pid={Environment.ProcessId} ", error.Message, StringComparison.Ordinal);
            Assert.EndsWith("(running); gave up waiting for the pid lock after 0 ms (--lock-timeout)", error.Message, StringComparison.Ordinal);
            Assert.Null(StorePidLock.StaleReason(directory, StorePidLock.ReadOwner(directory)));
        }
        Assert.Empty(Directory.GetFileSystemEntries(_directory));
    }

    [Fact]
    public void PidCleanupRetriesWindowsSharingErrorsButLeavesPermanentErrorsAlone()
    {
        const int sharingViolation = unchecked((int)0x80070020);
        var attempts = 0;
        var cleaned = StorePidLock.TryCleanup(() =>
        {
            if (++attempts == 1) throw new IOException("held by a reader", sharingViolation);
        }, trace: false);
        Assert.Equal(OperatingSystem.IsWindows(), cleaned);
        Assert.Equal(OperatingSystem.IsWindows() ? 2 : 1, attempts);

        attempts = 0;
        Assert.False(StorePidLock.TryCleanup(() =>
        {
            attempts++;
            throw new DirectoryNotFoundException();
        }, trace: false));
        Assert.Equal(1, attempts);
    }

    [Fact]
    public void WindowsPidCleanupCanMoveTheDirectoryAfterItsReaderCloses()
    {
        if (!OperatingSystem.IsWindows()) return;

        var directory = PlantPidLock(Store(), StoreLockOwner.Current("held"));
        var released = directory + ".released";
        using var reader = new FileStream(Path.Combine(directory, "owner"),
          FileMode.Open, FileAccess.Read, FileShare.Read);
        var attempts = 0;
        Assert.True(StorePidLock.TryCleanup(() =>
        {
            attempts++;
            try
            {
                Directory.Move(directory, released);
            }
            catch (IOException) when (attempts == 1)
            {
                reader.Dispose();
                throw;
            }
        }, trace: true));
        Assert.Equal(2, attempts);
        Assert.Equal("held", StorePidLock.ReadOwner(released)?.Token);
        Assert.False(Directory.Exists(directory));
    }

    [Fact]
    public void BreaksTheLockOfAnOwnerThatIsGone()
    {
        var store = Store();
        var directory = PlantPidLock(store, StoreLockOwner.Current("gone") with { Pid = GonePid });
        Assert.Equal(
          $"flock {store}.lock: unlocked\npid {directory}: locked by {StorePidLock.ReadOwner(directory)!.Describe()}; "
            + $"stale, the next writer breaks it: owner process {GonePid} is gone\n",
          StoreLock.Status([store]));
        using (var held = StoreLock.Acquire([store], LockMode.Exclusive, Immediately(StoreLockKind.Pid) with { Trace = true }))
        {
            Assert.Equal((uint)Environment.ProcessId, StorePidLock.ReadOwner(directory)!.Pid);
        }
        Assert.Empty(Directory.GetFileSystemEntries(_directory));
    }

    [Fact]
    public void LeavesAnOwnerlessDirectoryAloneForItsGracePeriod()
    {
        var store = Store();
        var directory = StorePidLock.LockDirectoryPath(store);
        Directory.CreateDirectory(directory);
        File.WriteAllText(Path.Combine(directory, "note"), "");
        var error = Assert.Throws<StoreLockedException>(
          () => StoreLock.Acquire([store], LockMode.Exclusive, Immediately(StoreLockKind.Pid)));
        Assert.Contains(
          $"is held by a directory without an owner record (removed once {directory} is 10 s old);",
          error.Message,
          StringComparison.Ordinal);
        Directory.SetLastWriteTimeUtc(directory, DateTime.UtcNow - TimeSpan.FromMinutes(1));
        Assert.Equal("no owner record for 10 s", StorePidLock.StaleReason(directory, null));
        using var held = StoreLock.Acquire([store], LockMode.Exclusive, Immediately(StoreLockKind.Pid));
        Assert.NotNull(StorePidLock.ReadOwner(directory));
    }

    [Fact]
    public void ALockThatChangedHandsIsNeitherBrokenNorReleased()
    {
        var store = Store();
        var directory = StorePidLock.LockDirectoryPath(store);
        var held = StoreLock.Acquire([store], LockMode.Exclusive, Immediately(StoreLockKind.Pid));
        var successor = StoreLockOwner.Current("successor");
        File.WriteAllText(Path.Combine(directory, "owner"), successor.ToRecord());
        // Expecting the previous owner, a breaker puts the successor's lock back.
        StorePidLock.BreakStale(directory, StoreLockOwner.Current("previous"));
        Assert.Equal("successor", StorePidLock.ReadOwner(directory)!.Token);
        held.Dispose();
        Assert.Equal("successor", StorePidLock.ReadOwner(directory)!.Token);
        Assert.Null(StorePidLock.StaleReason(Path.Combine(_directory, "missing.lock.d"), null));
    }

    [Fact]
    public void WaitsForALockReleasedMeanwhile()
    {
        var store = Store();
        var held = StoreLock.Acquire([store], LockMode.Exclusive, Immediately(StoreLockKind.Flock));
        using var release = Task.Delay(100).ContinueWith(_ => held.Dispose(), TaskScheduler.Default);
        using var acquired = StoreLock.Acquire(
          [store], LockMode.Exclusive, new StoreLockOptions { TimeoutMs = 10_000, Trace = true });
        Assert.Single(acquired.Paths);
    }

    private static string PlantPidLock(string store, StoreLockOwner owner)
    {
        var directory = StorePidLock.LockDirectoryPath(store);
        Directory.CreateDirectory(directory);
        File.WriteAllText(Path.Combine(directory, "owner"), owner.ToRecord());
        return directory;
    }
}
