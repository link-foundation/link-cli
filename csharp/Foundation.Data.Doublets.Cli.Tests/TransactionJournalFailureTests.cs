using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionJournalFailureTests
{
    private sealed class FaultLog(string path) : NamedTypesDecorator<uint>(path)
    {
        public int Remaining { get; set; }
        public bool After { get; set; }
        public bool FailRollback { get; set; }
        public override uint SetName(uint link, string name)
        {
            if (name.StartsWith("__transactions:rollback:", StringComparison.Ordinal) && FailRollback)
            {
                FailRollback = false;
                throw new IOException("injected rollback marker");
            }
            var fail = name.StartsWith("__transactions:transition:", StringComparison.Ordinal)
                && Remaining > 0 && --Remaining == 0;
            if (fail && !After) throw new IOException("injected journal before append");
            var result = base.SetName(link, name);
            if (fail) throw new IOException("injected journal after append");
            return result;
        }
    }

    [Theory]
    [InlineData(false, false, 1, false, false)]
    [InlineData(false, true, 1, false, false)]
    [InlineData(false, false, 2, false, false)]
    [InlineData(false, true, 2, false, false)]
    [InlineData(true, false, 1, false, false)]
    [InlineData(true, true, 1, false, false)]
    [InlineData(true, false, 2, false, false)]
    [InlineData(true, true, 2, false, false)]
    [InlineData(true, false, 1, true, false)]
    [InlineData(true, true, 1, true, false)]
    [InlineData(true, false, 2, true, false)]
    [InlineData(true, true, 2, true, false)]
    [InlineData(false, false, 1, true, true)]
    [InlineData(false, true, 1, true, true)]
    public void CascadingWritesRemainReversibleAfterJournalFailure(bool explicitTransaction, bool after, int failAt, bool rejectCommit, bool cleanupFailure)
    {
        var directory = Path.Combine(Path.GetTempPath(), "journal-failure-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        uint a, b, c, following;
        try
        {
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new FaultLog(logPath))
            using (var tx = new TransactionsDecorator(data, log))
            {
                a = tx.CreateAndUpdate(0u, 0u);
                b = tx.CreateAndUpdate(a, a);
                c = tx.CreateAndUpdate(b, b);
                using var transaction = explicitTransaction ? tx.BeginTransaction() : null;
                log.Remaining = failAt; log.After = after; log.FailRollback = cleanupFailure;
                var changed = new HashSet<uint>();
                var error = Assert.Throws<IOException>(() => tx.Delete(new[] { b }, (before, afterState) =>
                {
                    if (before is not null) changed.Add(before[0]);
                    return tx.Constants.Continue;
                }));
                Assert.StartsWith(cleanupFailure ? "injected rollback marker" : "injected journal", error.Message);
                Assert.True(changed.SetEquals([b, c]));
                if (rejectCommit)
                {
                    if (transaction is not null) Assert.Throws<InvalidOperationException>(() => transaction.Commit());
                    Assert.Throws<InvalidOperationException>(() => tx.Create(null, null));
                }
                if (cleanupFailure)
                {
                    Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
                    tx.Rollback();
                }
                transaction?.Rollback();
                Assert.Equal(3u, tx.Count());
                Assert.True(tx.Exists(a)); Assert.True(tx.Exists(b)); Assert.True(tx.Exists(c));
                Assert.Equal(a, tx.GetSource(b)); Assert.Equal(a, tx.GetTarget(b));
                Assert.Equal(b, tx.GetSource(c)); Assert.Equal(b, tx.GetTarget(c));
                tx.Rollback(); // Repeated rollback is harmless after completion.
                using (var next = tx.BeginTransaction()) next.Rollback();
                following = tx.CreateAndUpdate(c, c);
            }
            using var reopenedData = new NamedTypesDecorator<uint>(dataPath);
            using var reopenedLog = new NamedTypesDecorator<uint>(logPath);
            using var reopened = new TransactionsDecorator(reopenedData, reopenedLog);
            Assert.Equal(4u, reopened.Count());
            Assert.True(reopened.Exists(following));
            Assert.Equal(a, reopened.GetSource(b)); Assert.Equal(b, reopened.GetSource(c));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Theory]
    [InlineData(false, false)]
    [InlineData(false, true)]
    [InlineData(true, false)]
    [InlineData(true, true)]
    public void SingleWriteAppendFailureRollsBackAutomatically(bool update, bool after)
    {
        var directory = Path.Combine(Path.GetTempPath(), "journal-single-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data"));
            using var log = new FaultLog(Path.Combine(directory, "log"));
            using var tx = new TransactionsDecorator(data, log);
            var id = tx.CreateAndUpdate(11u, 12u);
            log.Remaining = 1; log.After = after;
            Assert.Throws<IOException>(() =>
            {
                if (update) tx.Update(new[] { id }, new[] { id, 21u, 22u }, null);
                else tx.Create(null, null);
            });
            Assert.Equal(1u, tx.Count());
            Assert.Equal(11u, tx.GetSource(id)); Assert.Equal(12u, tx.GetTarget(id));
            using var next = tx.BeginTransaction();
            next.Rollback();
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
