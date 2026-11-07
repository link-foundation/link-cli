using Platform.Data;
using Platform.Data.Doublets;
using Doublet = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionRetentionRollbackTests
{
    private static Doublet[] Rows(INamedTypesLinks<uint> links)
    {
        var any = links.Constants.Any;
        return links.All(new Doublet(any, any, any)).Select(row => new Doublet(row)).OrderBy(row => row.Index).ToArray();
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void BackgroundRetentionLeavesPendingTransactionsUnapplied(bool chunked)
    {
        var directory = Path.Combine(Path.GetTempPath(), "pending-retention-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log"));
            using var injected = new ManualResetEventSlim();
            var archive = Path.Combine(directory, "archive");
            TransactionsDecorator? owner = null;
            ITransaction<uint>? pending = null;
            var injectOnce = 0;
            void FlushData()
            {
                data.Flush();
                if (owner is null || Interlocked.Exchange(ref injectOnce, 1) != 0) return;
                // Start the next foreground transaction while the older commit
                // is being flushed. No sleeps or scheduler timing are needed.
                try
                {
                    pending = owner.BeginTransaction();
                    for (var i = 0; i < 3; i++) owner.Create();
                }
                finally { injected.Set(); }
            }
            LogRetentionPolicy policy = chunked ? new LogRetentionPolicy.Chunked(2, archive) : new LogRetentionPolicy.Sized(1);
            using var tx = new TransactionsDecorator(data, log, policy, CommitMode.Async, flushData: FlushData);
            owner = tx;
            var committed = tx.Create();
            Assert.True(injected.Wait(TimeSpan.FromSeconds(10)), "Background data flush did not run.");
            // Shutdown waits for the current work item, including its retention,
            // and only stops the worker; the caller still owns both stores.
            tx.Shutdown();
            using var transaction = Assert.IsAssignableFrom<ITransaction<uint>>(pending);
            Assert.Equal(4, Rows(data).Length);
            Assert.False(transaction.IsCommitted);
            Assert.Equal(1, tx.AppliedSequence);
            Assert.Equal(chunked ? 4 : 3, tx.Log.Count);
            Assert.False(Directory.Exists(archive));
            transaction.Rollback();
            Assert.Equal(committed, Assert.Single(Rows(data)).Index);
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Theory]
    [InlineData(false, "create")]
    [InlineData(false, "update")]
    [InlineData(false, "delete")]
    [InlineData(true, "create")]
    [InlineData(true, "update")]
    [InlineData(true, "delete")]
    public void RetentionPreservesRollbackReopenAndFollowingCommits(bool chunked, string operation)
    {
        var directory = Path.Combine(Path.GetTempPath(), "rollback-retention-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        var dataPath = Path.Combine(directory, "data"); var logPath = Path.Combine(directory, "log");
        var archive = Path.Combine(directory, "archive");
        try
        {
            Doublet[] expected;
            Guid aborted;
            uint following;
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            {
                for (uint id = 1; id <= 3; id++) data.CreateAndUpdate(id + 10, id + 20);
                data.Flush(); var before = Rows(data);
                LogRetentionPolicy policy = chunked ? new LogRetentionPolicy.Chunked(2, archive) : new LogRetentionPolicy.Sized(1);
                using var tx = new TransactionsDecorator(data, log, retentionPolicy: policy);
                using (var transaction = tx.BeginTransaction())
                {
                    aborted = transaction.Id;
                    foreach (var row in before)
                    {
                        switch (operation)
                        {
                            case "create": tx.Create(); break;
                            case "update": tx.Update(row.Index, row.Index + 100, row.Index + 200); break;
                            case "delete": tx.Delete(row.Index); break;
                        }
                    }
                    transaction.Rollback();
                }
                Assert.Equal(before, Rows(data));
                Assert.Equal(0, tx.AppliedSequence);
                Assert.True(tx.Log.Count <= 1);
                Assert.False(Directory.Exists(archive));
                following = tx.CreateAndUpdate(400u, 500u);
                expected = Rows(data);
            }
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var reopened = new TransactionsDecorator(data, log))
            { Assert.Equal(expected, Rows(reopened)); Assert.True(reopened.Exists(following)); }
            if (chunked)
            {
                var changes = Directory.GetFiles(archive).SelectMany(File.ReadAllLines).Select(line =>
                { Assert.True(Transition<uint>.TryParse(line, out var change)); return change; }).ToArray();
                // The chunk includes one aborted entry and the following create;
                // its update remains in the live log until the next chunk.
                Assert.Single(changes);
                Assert.All(changes, change => Assert.NotEqual(aborted, change.TransactionId));
            }
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
