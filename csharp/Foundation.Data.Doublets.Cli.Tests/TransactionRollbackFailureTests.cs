using Platform.Data;
using Platform.Data.Doublets;
using Platform.Delegates;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionRollbackFailureTests
{
    private sealed class Fault
    {
        public string? Point { get; set; }
        public void Check(string point)
        {
            if (Point != point) return;
            Point = null;
            throw new IOException("injected " + point);
        }
    }
    private sealed class Store(string path, Fault fault) : NamedTypesDecorator<uint>(path)
    {
        public override uint Update(IList<uint>? restriction, IList<uint>? substitution, WriteHandler<uint>? handler)
        {
            fault.Check("undo-before");
            var result = base.Update(restriction, substitution, handler);
            fault.Check("undo-after");
            return result;
        }
        public override uint SetName(uint link, string name)
        {
            var rollback = name.StartsWith("__transactions:rollback:", StringComparison.Ordinal);
            if (rollback) fault.Check("marker-before");
            var result = base.SetName(link, name);
            if (rollback) fault.Check("marker-after");
            return result;
        }
    }

    [Theory]
    [InlineData("undo-before", true)]
    [InlineData("undo-after", true)]
    [InlineData("data-before", true)]
    [InlineData("data-after", true)]
    [InlineData("marker-before", true)]
    [InlineData("marker-after", true)]
    [InlineData("log-before", true)]
    [InlineData("log-after", true)]
    [InlineData("undo-before", false)]
    [InlineData("undo-after", false)]
    [InlineData("data-before", false)]
    [InlineData("data-after", false)]
    [InlineData("marker-before", false)]
    [InlineData("marker-after", false)]
    [InlineData("log-before", false)]
    [InlineData("log-after", false)]
    public void FailedRollbackRequiresCleanupBeforeCommitOrWrite(string point, bool commit)
    {
        var directory = Path.Combine(Path.GetTempPath(), "rollback-failure-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        var dataPath = Path.Combine(directory, "data"); var logPath = Path.Combine(directory, "log");
        try
        {
            uint a, b, following;
            var fault = new Fault();
            using (var data = new Store(dataPath, fault))
            using (var log = new Store(logPath, fault))
            using (var tx = new TransactionsDecorator(data, log,
                flushData: () => { fault.Check("data-before"); data.Flush(); fault.Check("data-after"); },
                flushLog: () => { fault.Check("log-before"); log.Flush(); fault.Check("log-after"); }))
            {
                a = tx.CreateAndUpdate(11u, 12u);
                b = tx.CreateAndUpdate(21u, 22u);
                using var transaction = tx.BeginTransaction();
                tx.Update(new[] { a }, new[] { a, 31u, 32u }, null);
                tx.Update(new[] { b }, new[] { b, 41u, 42u }, null);
                fault.Point = point;
                Assert.Contains(point, Assert.Throws<IOException>(() => transaction.Rollback()).Message);
                Assert.False(transaction.IsCommitted); Assert.False(transaction.IsRolledBack);
                Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
                if (commit) Assert.Throws<InvalidOperationException>(() => transaction.Commit());
                else Assert.Throws<InvalidOperationException>(() => tx.Create(null, null));
                transaction.Rollback();
                Assert.True(transaction.IsRolledBack);
                Assert.Equal(2u, tx.Count());
                Assert.Equal(11u, tx.GetSource(a)); Assert.Equal(12u, tx.GetTarget(a));
                Assert.Equal(21u, tx.GetSource(b)); Assert.Equal(22u, tx.GetTarget(b));
                using (var next = tx.BeginTransaction()) next.Rollback();
                following = tx.CreateAndUpdate(51u, 52u);
            }
            using var reopenedData = new NamedTypesDecorator<uint>(dataPath);
            using var reopenedLog = new NamedTypesDecorator<uint>(logPath);
            using var reopened = new TransactionsDecorator(reopenedData, reopenedLog);
            Assert.Equal(3u, reopened.Count());
            Assert.True(reopened.Exists(following));
            Assert.Equal(11u, reopened.GetSource(a)); Assert.Equal(12u, reopened.GetTarget(a));
            Assert.Equal(21u, reopened.GetSource(b)); Assert.Equal(22u, reopened.GetTarget(b));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
