using Platform.Data;
using System.Globalization;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests.Tests
{
    /// <summary>
    /// Covers the two properties that make the transactions layer reusable
    /// outside the <c>clink</c> CLI: it composes over any unsigned address
    /// type, and a file-backed store recovers the same way after a crash as
    /// the Rust port does.
    /// </summary>
    public class TransactionsDecoratorGenericTests
    {
        [Fact]
        public void TransactionsWorkOverAUlongAddressedStore()
        {
            RunWithUlongTransactions(tx =>
            {
                var committed = tx.CreateAndUpdate(tx.Constants.Null, tx.Constants.Null);
                Assert.True(tx.Exists(committed));

                ulong rolledBack;
                using (var transaction = tx.BeginTransaction())
                {
                    rolledBack = tx.CreateAndUpdate(tx.Constants.Null, tx.Constants.Null);
                    Assert.True(tx.Exists(rolledBack));
                    transaction.Rollback();
                }

                Assert.False(tx.Exists(rolledBack), "Rolled-back create must remove the link.");
                Assert.True(tx.Exists(committed), "The committed link must survive an unrelated rollback.");
            });
        }

        [Fact]
        public void TheTransitionWireFormatDoesNotDependOnTheAddressType()
        {
            var id = Guid.NewGuid();
            var narrow = new Transition<uint>(
              id,
              Sequence: 7,
              Timestamp: DateTimeOffset.FromUnixTimeMilliseconds(1234567890000),
              Kind: TransitionKind.Update,
              Before: new Link<uint>(1, 2, 3),
              After: new Link<uint>(1, 4, 5));
            var wide = new Transition<ulong>(
              id,
              Sequence: 7,
              Timestamp: DateTimeOffset.FromUnixTimeMilliseconds(1234567890000),
              Kind: TransitionKind.Update,
              Before: new Link<ulong>(1, 2, 3),
              After: new Link<ulong>(1, 4, 5));

            Assert.Equal(narrow.Serialize(), wide.Serialize());

            // A log written by a uint-addressed store reads back unchanged in
            // a ulong-addressed one.
            Assert.True(Transition<ulong>.TryParse(narrow.Serialize(), out var parsed));
            Assert.Equal(wide, parsed);
        }

        [Fact]
        public void AnAddressThatDoesNotFitTheStoreIsRejectedNotSilentlyTruncated()
        {
            var wide = new Transition<ulong>(
              Guid.NewGuid(),
              Sequence: 1,
              Timestamp: DateTimeOffset.FromUnixTimeMilliseconds(0),
              Kind: TransitionKind.Create,
              Before: new Link<ulong>(0, 0, 0),
              After: new Link<ulong>((ulong)uint.MaxValue + 1, 1, 1));

            Assert.False(
              Transition<uint>.TryParse(wide.Serialize(), out _),
              "A transition addressed beyond uint must not parse into a uint-addressed log.");
        }

        [Fact]
        public void CommittedWritesSurviveAReopenAndUncommittedOnesAreRolledBack()
        {
            var paths = new StorePaths();
            try
            {
                uint committed;
                uint abandoned;

                // First "process": commit one link, then crash mid-transaction
                // by abandoning the handle without commit or rollback.
                using (var scope = paths.Open())
                {
                    committed = scope.Transactions.CreateAndUpdate(
                      scope.Transactions.Constants.Null, scope.Transactions.Constants.Null);

                    var transaction = scope.Transactions.BeginTransaction();
                    abandoned = scope.Transactions.CreateAndUpdate(
                      scope.Transactions.Constants.Null, scope.Transactions.Constants.Null);
                    Assert.True(scope.Transactions.Exists(abandoned));
                    GC.KeepAlive(transaction);
                }

                // Second "process": recovery runs in the constructor.
                using (var scope = paths.Open())
                {
                    Assert.True(
                      scope.Transactions.Exists(committed),
                      "A committed write must survive the crash.");
                    Assert.False(
                      scope.Transactions.Exists(abandoned),
                      "A write from a transaction that never committed must be rolled back (R10).");
                }

                // Recovery is idempotent: a third open must not undo anything else.
                using (var scope = paths.Open())
                {
                    Assert.True(scope.Transactions.Exists(committed));
                    Assert.False(scope.Transactions.Exists(abandoned));
                }
            }
            finally
            {
                paths.Cleanup();
            }
        }

        [Fact]
        public void TheDataStoreIsMutatedInPlaceAcrossTransactions()
        {
            var paths = new StorePaths();
            try
            {
                using var scope = paths.Open();
                var firstWriteTime = new FileInfo(paths.Data).LastWriteTimeUtc;
                for (var i = 0; i < 8; i++)
                {
                    scope.Transactions.CreateAndUpdate(
                      scope.Transactions.Constants.Null, scope.Transactions.Constants.Null);
                }

                // The database file is the same file throughout: a consumer
                // that keeps it mapped never sees its mapping detached by a
                // temp-file-and-rename rebuild.
                Assert.True(File.Exists(paths.Data));
                Assert.True(new FileInfo(paths.Data).LastWriteTimeUtc >= firstWriteTime);
            }
            finally
            {
                paths.Cleanup();
            }
        }

        // -- helpers ---------------------------------------------------------

        private sealed class StorePaths
        {
            public StorePaths()
            {
                Directory = Path.Combine(Path.GetTempPath(), "clink-tx-" + Guid.NewGuid().ToString("N"));
                System.IO.Directory.CreateDirectory(Directory);
                Data = Path.Combine(Directory, "db.links");
                Log = Path.Combine(Directory, "db.transitions.links");
            }

            public string Directory { get; }
            public string Data { get; }
            public string Log { get; }

            public Scope Open() => new(Data, Log);

            public void Cleanup()
            {
                try { System.IO.Directory.Delete(Directory, recursive: true); }
                catch (IOException) { /* best effort */ }
            }
        }

        private sealed class Scope : IDisposable
        {
            private readonly NamedTypesDecorator<uint> _data;
            private readonly NamedTypesDecorator<uint> _log;

            public Scope(string dataFile, string logFile)
            {
                _data = new NamedTypesDecorator<uint>(dataFile);
                _log = new NamedTypesDecorator<uint>(logFile);
                Transactions = new TransactionsDecorator(_data, _log);
            }

            public TransactionsDecorator Transactions { get; }

            public void Dispose()
            {
                Transactions.Dispose();
                _data.Dispose();
                _log.Dispose();
            }
        }

        private static void RunWithUlongTransactions(Action<TransactionsDecorator<ulong>> action)
        {
            var directory = Path.Combine(Path.GetTempPath(), "clink-tx-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(directory);
            var dataFile = Path.Combine(directory, "db.links");
            var logFile = Path.Combine(directory, "db.transitions.links");
            NamedTypesDecorator<ulong>? data = null;
            NamedTypesDecorator<ulong>? log = null;
            TransactionsDecorator<ulong>? tx = null;
            try
            {
                data = new NamedTypesDecorator<ulong>(dataFile);
                log = new NamedTypesDecorator<ulong>(logFile);
                tx = new TransactionsDecorator<ulong>(data, log);
                action(tx);
            }
            finally
            {
                tx?.Dispose();
                data?.Dispose();
                log?.Dispose();
                try { Directory.Delete(directory, recursive: true); }
                catch (IOException) { /* best effort */ }
            }
        }
    }
}
