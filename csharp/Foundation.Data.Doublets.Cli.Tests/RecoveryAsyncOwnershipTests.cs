using System.Reflection;
using System.Threading.Channels;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class RecoveryAsyncOwnershipTests
{
    private sealed class Journal(string path) : NamedTypesDecorator<uint>(path)
    {
        public bool FailRead;
        public override string? GetName(uint link)
        {
            if (FailRead)
            {
                FailRead = false;
                throw new IOException("injected recovery read");
            }
            return base.GetName(link);
        }
    }

    [Theory(Timeout = 20000)]
    [InlineData(false)]
    [InlineData(true)]
    public async Task QueuedCheckpointsAndRetentionWaitForFailedRecovery(bool chunked)
    {
        var directory = Directory.CreateTempSubdirectory("recovery-async-owner-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        var archivePath = Path.Combine(directory, "archive");
        uint id;
        try
        {
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new Journal(logPath))
            using (var tx = new TransactionsDecorator(data, log,
                retentionPolicy: chunked ? new LogRetentionPolicy.Chunked(1, archivePath) : new LogRetentionPolicy.Sized(1),
                commitMode: CommitMode.Async))
            {
                // Hold admission solely to schedule queued work before the
                // recovery failure, without racing a worker or adding sleeps.
                var gate = typeof(TransactionsDecorator<uint>).GetField("_lock", BindingFlags.NonPublic | BindingFlags.Instance)!.GetValue(tx)!;
                var queue = (Channel<Action>)typeof(TransactionsDecorator<uint>).GetField("_asyncQueue", BindingFlags.NonPublic | BindingFlags.Instance)!.GetValue(tx)!;
                var drained = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
                string?[] names;
                int retained;
                string?[] Names() => log.All(new[] { log.Constants.Any, log.Constants.Any, log.Constants.Any })
                    .Select(row => log.GetName(Assert.IsAssignableFrom<IList<uint>>(row)[0])).ToArray();
                lock (gate)
                {
                    id = tx.CreateAndUpdate(11u, 12u);
                    tx.Update(new[] { id }, new[] { id, 21u, 22u }, null);
                    names = Names();
                    retained = tx.Log.Count;
                    Assert.True(retained > 1);
                    log.FailRead = true;
                    Assert.Throws<IOException>(tx.Recover);
                    Assert.True(tx.HasOpenTransaction);
                    Assert.True(queue.Writer.TryWrite(() => drained.TrySetResult()));
                }
                // A sentinel follows the real queued work, proving it ran
                // before shutdown can cancel anything. Reflection controls
                // scheduling only; assertions exercise public state and stores.
                await drained.Task.WaitAsync(TimeSpan.FromSeconds(5));
                tx.Shutdown();
                Assert.Equal(names, Names());
                Assert.Equal(retained, tx.Log.Count);
                Assert.False(Directory.Exists(archivePath));
                Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
                Assert.Throws<InvalidOperationException>(tx.BeginVersionReplay);
                tx.Recover();
                Assert.False(tx.HasOpenTransaction);
                Assert.Equal(21u, tx.GetSource(id));
                Assert.Equal(22u, tx.GetTarget(id));
                Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
                tx.CommitMode = CommitMode.Sync;
                tx.Update(new[] { id }, new[] { id, 41u, 42u }, null);
            }
            for (var reopen = 0; reopen < 2; reopen++)
            {
                using var data = new NamedTypesDecorator<uint>(dataPath);
                using var log = new NamedTypesDecorator<uint>(logPath);
                using var tx = new TransactionsDecorator(data, log);
                Assert.Equal(41u, tx.GetSource(id));
                Assert.Equal(42u, tx.GetTarget(id));
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
