using Foundation.Data.Doublets.Cli.Engine;
using IsolationLevel = Foundation.Data.Doublets.Cli.Engine.IsolationLevel;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>Bounded waiting and deadlock refusal of pessimistic point writes.</summary>
public class EngineLockWaitTests
{
    private static readonly TimeSpan Long = TimeSpan.FromSeconds(60);

    private static LinksEngine<uint> Waiting(TimeSpan wait)
    {
        var db = new LinksEngine<uint>(new MemoryCommitLog(), new EngineOptions
        {
            Concurrency = ConcurrencyControl.Pessimistic,
            Isolation = IsolationLevel.Snapshot,
            LockWait = wait,
        });
        using var seed = db.Begin();
        for (uint id = 1; id <= 3; id++) seed.EnsureCreated(id);
        seed.Commit();
        return db;
    }

    /// <summary>Runs a write on its own thread, returning once it has had time to block.</summary>
    private static async Task<Task<T>> Blocked<T>(Func<T> write)
    {
        var task = Task.Factory.StartNew(write, TaskCreationOptions.LongRunning);
        await Task.Delay(50);
        return task;
    }

    [Fact]
    public async Task AWriteWaitsForTheOwnerToRollBackAndThenSucceeds()
    {
        var db = Waiting(Long);
        var owner = db.Begin();
        owner.SetName(1, "private");
        using var waiter = db.Begin();
        var write = await Blocked(() => waiter.SetName(1, "next"));
        Assert.False(write.IsCompleted);
        owner.Rollback();
        Assert.Equal(1u, await write);
        waiter.Commit();
        using var read = db.Begin();
        Assert.Equal(1u, read.GetByName("next"));
    }

    [Fact]
    public async Task AWriteThatWaitedForACommitIsAConflictLikeAnyStaleWrite()
    {
        var db = Waiting(Long);
        var owner = db.Begin();
        owner.SetName(1, "first");
        using var waiter = db.Begin();
        var write = await Blocked(() => waiter.SetName(1, "second"));
        Assert.False(write.IsCompleted);
        owner.Commit();
        var refused = await Assert.ThrowsAsync<InvalidOperationException>(() => write);
        Assert.StartsWith("Conflict", refused.Message);
    }

    [Fact]
    public void AWaitEndsWithBusyAtItsLimitAndKeepsEarlierStagedWrites()
    {
        var db = Waiting(TimeSpan.FromMilliseconds(30));
        using var owner = db.Begin();
        owner.SetName(1, "owned");
        using var waiter = db.Begin();
        waiter.SetName(2, "kept");
        var refused = Assert.Throws<InvalidOperationException>(() => waiter.RemoveName(1));
        Assert.StartsWith("Busy: timed out", refused.Message);
        Assert.Equal(2u, waiter.GetByName("kept"));
        owner.Rollback();
        waiter.SetName(1, "after");
        waiter.Commit();
    }

    [Fact]
    public void AZeroWaitAnswersAtOnceAndAnOwnerNeverWaitsForItself()
    {
        var db = Waiting(TimeSpan.Zero);
        using var owner = db.Begin();
        owner.SetName(1, "owned");
        using var other = db.Begin();
        Assert.StartsWith("Busy", Assert.Throws<InvalidOperationException>(() => other.UpdateLink(1, 1, 1)).Message);
        owner.SetName(1, "again");
    }

    [Fact]
    public async Task TheWriteThatClosesACycleIsRefusedAndTheOtherContinues()
    {
        var db = Waiting(Long);
        var first = db.Begin();
        var second = db.Begin();
        first.SetName(1, "one");
        second.SetName(2, "two");
        var write = await Blocked(() => first.SetName(2, "first"));
        var refused = Assert.Throws<InvalidOperationException>(() => second.RemoveName(1));
        Assert.StartsWith("Deadlock", refused.Message);
        Assert.Equal(2u, second.GetByName("two"));
        second.Rollback();
        Assert.Equal(2u, await write);
        first.Commit();
    }

    [Fact]
    public async Task ACycleThroughThreeSessionsIsRefused()
    {
        var db = Waiting(Long);
        using var first = db.Begin();
        using var second = db.Begin();
        using var third = db.Begin();
        first.SetName(1, "one");
        second.SetName(2, "two");
        third.SetName(3, "three");
        var a = await Blocked(() => { first.RemoveName(2); first.Rollback(); return true; });
        var b = await Blocked(() => { second.RemoveName(3); second.Rollback(); return true; });
        Assert.StartsWith("Deadlock", Assert.Throws<InvalidOperationException>(() => third.RemoveName(1)).Message);
        third.Rollback();
        Assert.True(await b);
        Assert.True(await a);
    }

    [Fact]
    public async Task ExplicitCreationWaitsAndNeverMovesTheAllocatorBack()
    {
        var db = Waiting(Long);
        var owner = db.Begin();
        owner.EnsureCreated(10);
        using var waiter = db.Begin();
        var write = await Blocked(() => waiter.EnsureCreated(10));
        Assert.False(write.IsCompleted);
        using (var other = db.Begin()) { other.EnsureCreated(20); other.Commit(); }
        owner.Rollback();
        Assert.Equal(10u, await write);
        waiter.Commit();
        using var next = db.Begin();
        Assert.Equal(21u, next.CreateLink(1, 1));
    }

    [Fact]
    public async Task AReservedCreationWaitsForAnExplicitOwnerOfThatAddress()
    {
        var db = Waiting(Long);
        using var waiter = db.Begin();
        var reserved = waiter.ReserveQueryAllocations(preview => preview.Create(null, null))!.Max();
        var owner = db.Begin();
        owner.EnsureCreated(reserved);
        var write = await Blocked(() => waiter.CreateLink(1, 1));
        Assert.False(write.IsCompleted);
        owner.Rollback();
        Assert.Equal(reserved, await write);
    }

    [Fact]
    public async Task AGroupCommitWakesWaiters()
    {
        var db = Waiting(Long);
        var owner = db.Begin();
        owner.SetName(1, "grouped");
        using var waiter = db.Begin();
        var write = await Blocked(() => { waiter.RemoveName(1); return true; });
        Assert.False(write.IsCompleted);
        Assert.Equal(GroupCommitStatus.Committed, Assert.Single(db.CommitGroup([owner])).Status);
        Assert.StartsWith("Conflict", (await Assert.ThrowsAsync<InvalidOperationException>(() => write)).Message);
    }

    [Fact]
    public async Task AWaiterStopsWhenItsSessionIsClosedFromAnotherThread()
    {
        var db = Waiting(Long);
        using var owner = db.Begin();
        owner.SetName(1, "owned");
        var waiter = db.Begin();
        var write = await Blocked(() => waiter.SetName(1, "next"));
        waiter.Rollback();
        Assert.Equal("Transaction is closed.", (await Assert.ThrowsAsync<InvalidOperationException>(() => write)).Message);
    }

    [Fact]
    public void LockWaitingRequiresPessimisticConcurrencyAndAForwardLimit()
    {
        foreach (var concurrency in new[] { ConcurrencyControl.Optimistic, ConcurrencyControl.Exclusive })
            Assert.Contains("pessimistic", Assert.Throws<ArgumentException>(() => new LinksEngine<uint>(new MemoryCommitLog(),
                new EngineOptions { Concurrency = concurrency, LockWait = Long })).Message);
        Assert.Throws<ArgumentException>(() => new LinksEngine<uint>(new MemoryCommitLog(),
            new EngineOptions { Concurrency = ConcurrencyControl.Pessimistic, LockWait = TimeSpan.FromSeconds(-1) }));
    }

    private sealed class FlushFault : ICommitLog
    {
        private readonly MemoryCommitLog _inner = new();
        public bool Fail;
        public bool SupportsDurableFlush => true;
        public IReadOnlyList<string> Read() => _inner.Read();
        public void Append(string record) => _inner.Append(record);
        public void Flush() { if (Fail) throw new IOException("flush"); }
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task AnUnknownJournalOutcomeEndsAWaitWithTheReopenError(bool grouped)
    {
        var log = new FlushFault();
        var db = new LinksEngine<uint>(log, new EngineOptions
        {
            Concurrency = ConcurrencyControl.Pessimistic,
            Isolation = IsolationLevel.Snapshot,
            LockWait = Long,
        });
        using (var seed = db.Begin()) { seed.EnsureCreated(1); seed.Commit(); }
        var owner = db.Begin();
        owner.SetName(1, "private");
        using var waiter = db.Begin();
        var write = await Blocked(() => waiter.SetName(1, "next"));
        log.Fail = true;
        if (grouped) db.CommitGroup(new[] { owner });
        else Assert.ThrowsAny<Exception>(() => owner.Commit());
        var finished = await Task.WhenAny(write, Task.Delay(TimeSpan.FromSeconds(30)));
        Assert.Same(write, finished);
        var refused = await Assert.ThrowsAsync<InvalidOperationException>(() => write);
        Assert.Contains("reopen", refused.Message);
    }
}
