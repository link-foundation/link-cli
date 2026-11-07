using System.Reflection;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class AsyncShutdownTests
{
    [Theory(Timeout = 30000)]
    [InlineData(1)]
    [InlineData(2)]
    public async Task EveryShutdownCallerWaitsForInFlightStorageUse(int callers)
    {
        var directory = Directory.CreateTempSubdirectory("async-shutdown-").FullName;
        using var entered = new ManualResetEventSlim();
        using var release = new ManualResetEventSlim();
        var armed = false;
        var stopping = new List<Task>();
        using var started = new CountdownEvent(callers);
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            void Flush()
            {
                if (Volatile.Read(ref armed))
                {
                    entered.Set();
                    if (!release.Wait(TimeSpan.FromSeconds(15)))
                        throw new IOException("Bounded flush gate expired.");
                }
                data.Flush();
            }
            using var tx = new TransactionsDecorator(data, log, commitMode: CommitMode.Async,
                flushData: Flush, flushLog: log.Flush);
            var worker = WorkerOf(tx);
            try
            {
                Volatile.Write(ref armed, true);
                tx.Create(null, null);
                Assert.True(entered.Wait(TimeSpan.FromSeconds(5)), "Worker did not reach its data flush.");
                for (var i = 0; i < callers; i++)
                    stopping.Add(Task.Factory.StartNew(() =>
                    {
                        started.Signal();
                        tx.Shutdown();
                    }, CancellationToken.None, TaskCreationOptions.LongRunning, TaskScheduler.Default));
                Assert.True(started.Wait(TimeSpan.FromSeconds(5)), "Shutdown callers did not start.");
                // Pass the previous best-effort five-second join while the
                // bounded gate keeps the persistence callback in flight.
                await Task.Delay(TimeSpan.FromSeconds(6));
                Assert.All(stopping, task => Assert.False(task.IsCompleted,
                    "Shutdown returned while borrowed storage was still in use."));
            }
            finally
            {
                release.Set();
                // Join the actual worker even when the old Shutdown returned
                // early. Reflection is solely failing-test resource cleanup;
                // the assertion above exercises the public lifetime contract.
                await worker.WaitAsync(TimeSpan.FromSeconds(5));
                await Task.WhenAll(stopping).WaitAsync(TimeSpan.FromSeconds(5));
            }
            tx.Shutdown(); // Completed shutdown is idempotent.
            Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
        }
        finally
        {
            release.Set();
            Directory.Delete(directory, recursive: true);
        }
    }

    [Theory(Timeout = 20000)]
    [InlineData(false)]
    [InlineData(true)]
    public async Task WorkerCallbackCannotJoinItsOwnShutdown(bool fromChildTask)
    {
        var directory = Directory.CreateTempSubdirectory("async-self-shutdown-").FullName;
        using var attempted = new ManualResetEventSlim();
        TransactionsDecorator? owner = null;
        Exception? error = null;
        var armed = false;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            void Flush()
            {
                if (Volatile.Read(ref armed))
                {
                    error = Record.Exception(() =>
                    {
                        if (fromChildTask) Task.Run(owner!.Shutdown).GetAwaiter().GetResult();
                        else owner!.Shutdown();
                    });
                    attempted.Set();
                }
                data.Flush();
            }
            using var tx = new TransactionsDecorator(data, log, commitMode: CommitMode.Async,
                flushData: Flush, flushLog: log.Flush);
            owner = tx;
            var worker = WorkerOf(tx);
            try
            {
                Volatile.Write(ref armed, true);
                tx.Create(null, null);
                Assert.True(attempted.Wait(TimeSpan.FromSeconds(8)), "Worker callback did not return.");
                Assert.IsType<InvalidOperationException>(error);
            }
            finally
            {
                Volatile.Write(ref armed, false);
                tx.Shutdown();
                await worker.WaitAsync(TimeSpan.FromSeconds(5));
            }
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact(Timeout = 15000)]
    public async Task SynchronousFlushCannotShutdownWhileHoldingStorageLock()
    {
        await Task.CompletedTask;
        var directory = Directory.CreateTempSubdirectory("sync-self-shutdown-").FullName;
        TransactionsDecorator? owner = null;
        Exception? error = null;
        var armed = false;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            void Flush()
            {
                if (armed) error = Record.Exception(owner!.Shutdown);
                data.Flush();
            }
            using var tx = new TransactionsDecorator(data, log, flushData: Flush, flushLog: log.Flush);
            owner = tx;
            armed = true;
            tx.Create(null, null);
            armed = false;
            Assert.IsType<InvalidOperationException>(error);
            Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact(Timeout = 15000)]
    public async Task DetachedCallbackTaskCanShutdownAfterWorkerHasStopped()
    {
        var directory = Directory.CreateTempSubdirectory("detached-shutdown-").FullName;
        using var scheduled = new ManualResetEventSlim();
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        Task? descendant = null;
        TransactionsDecorator? owner = null;
        var armed = false;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            void Flush()
            {
                if (Volatile.Read(ref armed))
                {
                    descendant = Task.Run(async () =>
                    {
                        await release.Task.WaitAsync(TimeSpan.FromSeconds(10));
                        owner!.Shutdown();
                    });
                    scheduled.Set();
                }
                data.Flush();
            }
            using var tx = new TransactionsDecorator(data, log, commitMode: CommitMode.Async,
                flushData: Flush, flushLog: log.Flush);
            owner = tx;
            try
            {
                Volatile.Write(ref armed, true);
                tx.Create(null, null);
                Assert.True(scheduled.Wait(TimeSpan.FromSeconds(5)));
                tx.Shutdown();
                release.SetResult();
                await descendant!.WaitAsync(TimeSpan.FromSeconds(5));
                Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
            }
            finally
            {
                release.TrySetResult();
                if (descendant is not null) await descendant.WaitAsync(TimeSpan.FromSeconds(5));
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact(Timeout = 15000)]
    public async Task FailedAsyncCheckpointRemainsRecoverableAfterShutdown()
    {
        await Task.CompletedTask;
        var directory = Directory.CreateTempSubdirectory("failed-checkpoint-shutdown-").FullName;
        using var attempted = new ManualResetEventSlim();
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        uint created;
        var armed = false;
        try
        {
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            {
                void Flush()
                {
                    if (Volatile.Read(ref armed))
                    {
                        attempted.Set();
                        throw new IOException("Injected asynchronous checkpoint failure.");
                    }
                    data.Flush();
                }
                using var tx = new TransactionsDecorator(data, log, commitMode: CommitMode.Async,
                    trace: true, flushData: Flush, flushLog: log.Flush);
                Volatile.Write(ref armed, true);
                using (var transaction = tx.BeginTransaction())
                {
                    created = tx.CreateAndUpdate(0u, 0u);
                    transaction.Commit();
                }
                Assert.True(attempted.Wait(TimeSpan.FromSeconds(5)));
                tx.Shutdown();
                Assert.Equal(0, tx.AppliedSequence);
            }
            using var reopenedData = new NamedTypesDecorator<uint>(dataPath);
            using var reopenedLog = new NamedTypesDecorator<uint>(logPath);
            using var recovered = new TransactionsDecorator(reopenedData, reopenedLog);
            Assert.True(recovered.Exists(created));
            Assert.Equal(recovered.LastLoggedSequence, recovered.AppliedSequence);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    private static Task WorkerOf(TransactionsDecorator owner) => (Task)typeof(TransactionsDecorator<uint>)
        .GetField("_backgroundWorker", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(owner)!;
}
