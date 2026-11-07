using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class AsyncRecoveryOrderingTests
{
    [Theory(Timeout = 20000)]
    [InlineData("update")]
    [InlineData("delete")]
    [InlineData("recreate")]
    public async Task LaterAppliedCommitSurvivesOlderFailedCheckpoint(string operation)
    {
        await Task.CompletedTask;
        var directory = Directory.CreateTempSubdirectory("async-recovery-order-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        var attempted = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var failNext = 0;
        uint id;
        long olderSequence;
        try
        {
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var tx = new TransactionsDecorator(data, log, flushData: () =>
            {
                if (Interlocked.Exchange(ref failNext, 0) == 1)
                {
                    attempted.TrySetResult();
                    throw new IOException("injected older asynchronous checkpoint");
                }
                data.Flush();
            }))
            {
                id = tx.CreateAndUpdate(11u, 12u);
                tx.CommitMode = CommitMode.Async;
                olderSequence = tx.LastLoggedSequence + 1;
                Volatile.Write(ref failNext, 1);
                using (var older = tx.BeginTransaction())
                {
                    if (operation == "recreate") tx.Delete(new[] { id }, null);
                    else tx.Update(new[] { id }, new[] { id, 21u, 22u }, null);
                    older.Commit();
                }
                await attempted.Task.WaitAsync(TimeSpan.FromSeconds(5));
                tx.CommitMode = CommitMode.Sync;
                using (var newer = tx.BeginTransaction())
                {
                    if (operation == "delete") tx.Delete(new[] { id }, null);
                    else
                    {
                        if (operation == "recreate") tx.EnsureCreated(id);
                        tx.Update(new[] { id }, new[] { id, 31u, 32u }, null);
                    }
                    newer.Commit();
                }
                tx.Shutdown();
                var any = log.Constants.Any;
                var names = log.All(new[] { any, any, any }).Select(row => log.GetName(Assert.IsAssignableFrom<IList<uint>>(row)[0])).ToArray();
                Assert.DoesNotContain("__transactions:applied:" + olderSequence, names);
                Assert.Contains("__transactions:applied:" + tx.LastLoggedSequence, names);
            }
            for (var reopen = 0; reopen < 2; reopen++)
            {
                using var data = new NamedTypesDecorator<uint>(dataPath);
                using var log = new NamedTypesDecorator<uint>(logPath);
                using var recovered = new TransactionsDecorator(data, log);
                if (operation == "delete") Assert.False(recovered.Exists(id));
                else
                {
                    Assert.True(recovered.Exists(id));
                    Assert.Equal(31u, recovered.GetSource(id));
                    Assert.Equal(32u, recovered.GetTarget(id));
                }
                Assert.Equal(recovered.LastLoggedSequence, recovered.AppliedSequence);
                var any = log.Constants.Any;
                Assert.Contains(log.All(new[] { any, any, any }), row => row is not null && log.GetName(row[0]) == "__transactions:applied:" + olderSequence);
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
