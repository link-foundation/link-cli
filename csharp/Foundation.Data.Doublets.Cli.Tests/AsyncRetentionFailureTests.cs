using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class AsyncRetentionFailureTests
{
    [Fact(Timeout = 20000)]
    public async Task AFailedBackgroundRetentionKeepsTheWorkerAndTheCommittedData()
    {
        var directory = Directory.CreateTempSubdirectory("async-retention-failure-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        var failures = 0;
        var failing = 0;
        uint first, second;
        try
        {
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var tx = new TransactionsDecorator(data, log, flushData: () =>
            {
                if (Volatile.Read(ref failing) == 1)
                {
                    // Once for the commit's own checkpoint, then for retention,
                    // which runs outside the per-record guard.
                    Interlocked.Increment(ref failures);
                    throw new IOException("injected background flush failure");
                }
                data.Flush();
            }))
            {
                tx.RetentionPolicy = new LogRetentionPolicy.Sized(1);
                tx.CommitMode = CommitMode.Async;
                Volatile.Write(ref failing, 1);
                first = tx.CreateAndUpdate(11u, 12u);
                second = tx.CreateAndUpdate(21u, 22u);
                using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(15));
                while (Volatile.Read(ref failures) < 4)
                    await Task.Delay(10, deadline.Token);

                // The worker survived: a later commit is still checkpointed.
                Volatile.Write(ref failing, 0);
                var third = tx.CreateAndUpdate(31u, 32u);
                tx.Shutdown();
                Assert.True(tx.Exists(first));
                Assert.True(tx.Exists(second));
                Assert.True(tx.Exists(third));
                Assert.True(tx.Log.Count <= 1);
            }

            using var reopenedData = new NamedTypesDecorator<uint>(dataPath);
            using var reopenedLog = new NamedTypesDecorator<uint>(logPath);
            using var reopened = new TransactionsDecorator(reopenedData, reopenedLog);
            Assert.True(reopened.Exists(first));
            Assert.True(reopened.Exists(second));
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }
}
