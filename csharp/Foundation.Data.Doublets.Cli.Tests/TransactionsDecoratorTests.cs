using Platform.Data;
using Platform.Data.Doublets;

using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
    public class TransactionsDecoratorTests
    {
        [Fact]
        public void AutoTransactionRecordsCreateAndUpdate()
        {
            // CreateAndUpdate is an extension that calls Create then Update on
            // the doublets store. Each emits a transition.
            RunWithTransactions((tx, _) =>
            {
                var created = tx.CreateAndUpdate(tx.Constants.Null, tx.Constants.Null);
                Assert.NotEqual(tx.Constants.Null, created);

                var log = tx.Log;
                Assert.Equal(2, log.Count);
                Assert.Equal(TransitionKind.Create, log[0].Kind);
                Assert.Equal(TransitionKind.Update, log[1].Kind);
                Assert.Equal(created, log[0].After.Index);
            });
        }

        [Fact]
        public void RollbackUndoesCreate()
        {
            RunWithTransactions((tx, _) =>
            {
                uint created;
                using (var transaction = tx.BeginTransaction())
                {
                    created = tx.CreateAndUpdate(tx.Constants.Null, tx.Constants.Null);
                    Assert.True(tx.Exists(created));
                    transaction.Rollback();
                }

                Assert.False(tx.Exists(created), "Rolled-back create must remove the link.");
            });
        }

        [Fact]
        public void DisposeWithoutCommitRollsBack()
        {
            RunWithTransactions((tx, _) =>
            {
                uint created;
                using (var transaction = tx.BeginTransaction())
                {
                    created = tx.CreateAndUpdate(tx.Constants.Null, tx.Constants.Null);
                }

                Assert.False(tx.Exists(created), "Disposing an open transaction must rollback (R10).");
            });
        }

        [Fact]
        public void CommitPersistsCreate()
        {
            RunWithTransactions((tx, _) =>
            {
                uint created;
                using (var transaction = tx.BeginTransaction())
                {
                    created = tx.CreateAndUpdate(tx.Constants.Null, tx.Constants.Null);
                    transaction.Commit();
                }

                Assert.True(tx.Exists(created));
                Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
            });
        }

        [Fact]
        public void RollbackUndoesUpdate()
        {
            RunWithTransactions((tx, _) =>
            {
                var a = tx.CreateAndUpdate(tx.Constants.Null, tx.Constants.Null);
                var b = tx.CreateAndUpdate(tx.Constants.Null, tx.Constants.Null);
                var c = tx.CreateAndUpdate(tx.Constants.Null, tx.Constants.Null);

                using (var transaction = tx.BeginTransaction())
                {
                    tx.Update(
                new DoubletLink(c, tx.Constants.Any, tx.Constants.Any),
                new DoubletLink(c, a, b),
                null);
                    var updated = new DoubletLink(tx.GetLink(c));
                    Assert.Equal(a, updated.Source);
                    Assert.Equal(b, updated.Target);
                    transaction.Rollback();
                }

                var afterRollback = new DoubletLink(tx.GetLink(c));
                Assert.Equal(c, afterRollback.Index);
                Assert.Equal(tx.Constants.Null, afterRollback.Source);
                Assert.Equal(tx.Constants.Null, afterRollback.Target);
            });
        }

        [Fact]
        public void RollbackUndoesDelete()
        {
            RunWithTransactions((tx, _) =>
            {
                var a = tx.CreateAndUpdate(tx.Constants.Null, tx.Constants.Null);
                var b = tx.CreateAndUpdate(tx.Constants.Null, tx.Constants.Null);
                var c = tx.CreateAndUpdate(tx.Constants.Null, tx.Constants.Null);
                tx.Update(
            new DoubletLink(c, tx.Constants.Any, tx.Constants.Any),
            new DoubletLink(c, a, b),
            null);

                using (var transaction = tx.BeginTransaction())
                {
                    tx.Delete(new DoubletLink(c, tx.Constants.Any, tx.Constants.Any), null);
                    Assert.False(tx.Exists(c));
                    transaction.Rollback();
                }

                Assert.True(tx.Exists(c), "Delete must be restored by rollback.");
                var restored = new DoubletLink(tx.GetLink(c));
                Assert.Equal(a, restored.Source);
                Assert.Equal(b, restored.Target);
            });
        }

        [Fact]
        public void SizedRetentionDropsOldestAfterApplied()
        {
            RunWithTransactions((tx, _) =>
            {
                tx.RetentionPolicy = new LogRetentionPolicy.Sized(MaxTransitions: 3);
                for (var i = 0; i < 5; i++)
                {
                    tx.CreateAndUpdate(tx.Constants.Null, tx.Constants.Null);
                }

                Assert.True(tx.Log.Count <= 3, $"Sized retention must cap log length; got {tx.Log.Count}.");
            });
        }

        [Fact]
        public void ChunkedRetentionArchivesOldest()
        {
            var archiveDir = Path.Combine(Path.GetTempPath(), $"tx-archive-{Guid.NewGuid():N}");
            try
            {
                RunWithTransactions((tx, _) =>
                {
                    tx.RetentionPolicy = new LogRetentionPolicy.Chunked(ChunkSize: 2, ArchiveDirectory: archiveDir);
                    for (var i = 0; i < 4; i++)
                    {
                        tx.CreateAndUpdate(tx.Constants.Null, tx.Constants.Null);
                    }

                    Assert.True(Directory.Exists(archiveDir));
                    var files = Directory.EnumerateFiles(archiveDir, "transitions-chunk-*.log").ToList();
                    Assert.NotEmpty(files);
                });
            }
            finally
            {
                if (Directory.Exists(archiveDir)) Directory.Delete(archiveDir, recursive: true);
            }
        }

        [Fact]
        public void RetentionPolicyParsesSpecs()
        {
            Assert.IsType<LogRetentionPolicy.Infinite>(LogRetentionPolicy.Parse("infinite"));
            Assert.IsType<LogRetentionPolicy.Sized>(LogRetentionPolicy.Parse("sized:1000"));
            Assert.IsType<LogRetentionPolicy.Chunked>(LogRetentionPolicy.Parse("chunked:500:/tmp/x"));
            Assert.Throws<ArgumentException>(() => LogRetentionPolicy.Parse("garbage"));
        }

        [Fact]
        public void TransitionRoundTripsThroughSerialize()
        {
            var t = new Transition(
              Guid.NewGuid(),
              Sequence: 42,
              Timestamp: DateTimeOffset.FromUnixTimeMilliseconds(1234567890000),
              Kind: TransitionKind.Update,
              Before: new DoubletLink(1, 2, 3),
              After: new DoubletLink(1, 4, 5));

            Assert.True(Transition.TryParse(t.Serialize(), out var parsed));
            Assert.Equal(t, parsed);
        }

        [Fact]
        public void AsyncCommitEventuallyMarksApplied()
        {
            RunWithTransactions((tx, _) =>
            {
                tx.CommitMode = CommitMode.Async;
                uint created;
                using (var transaction = tx.BeginTransaction())
                {
                    created = tx.CreateAndUpdate(tx.Constants.Null, tx.Constants.Null);
                    transaction.CommitAsync().GetAwaiter().GetResult();
                }

                // Allow background worker time to drain.
                var deadline = DateTime.UtcNow.AddSeconds(5);
                while (tx.AppliedSequence < tx.LastLoggedSequence && DateTime.UtcNow < deadline)
                {
                    Thread.Sleep(50);
                }
                Assert.Equal(tx.LastLoggedSequence, tx.AppliedSequence);
                Assert.True(tx.Exists(created));
            });
        }

        [Fact]
        public void NoBehaviourChangeWhenNotOptedIn()
        {
            // Acceptance for R8: bare NamedTypesDecorator behaves identically
            // whether or not TransactionsDecorator is wrapped above it.
            var dataFile = Path.GetTempFileName();
            NamedTypesDecorator<uint>? dataLinks = null;
            try
            {
                dataLinks = new NamedTypesDecorator<uint>(dataFile);
                var created = dataLinks.CreateAndUpdate(dataLinks.Constants.Null, dataLinks.Constants.Null);
                Assert.True(dataLinks.Exists(created));
            }
            finally
            {
                dataLinks?.Dispose();
                Cleanup(dataFile);
                Cleanup(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(dataFile));
            }
        }

        private static void RunWithTransactions(Action<TransactionsDecorator, NamedTypesDecorator<uint>> action)
        {
            var dataFile = Path.GetTempFileName();
            var logFile = Path.GetTempFileName();
            NamedTypesDecorator<uint>? dataLinks = null;
            NamedTypesDecorator<uint>? logLinks = null;
            TransactionsDecorator? tx = null;
            try
            {
                dataLinks = new NamedTypesDecorator<uint>(dataFile);
                logLinks = new NamedTypesDecorator<uint>(logFile);
                tx = new TransactionsDecorator(dataLinks, logLinks);
                action(tx, dataLinks);
            }
            finally
            {
                tx?.Dispose();
                dataLinks?.Dispose();
                logLinks?.Dispose();
                Cleanup(dataFile);
                Cleanup(logFile);
                Cleanup(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(dataFile));
                Cleanup(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(logFile));
            }
        }

        private static void Cleanup(string path)
        {
            if (File.Exists(path)) File.Delete(path);
        }
    }
}
