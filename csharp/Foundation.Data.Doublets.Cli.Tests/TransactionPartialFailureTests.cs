using Platform.Data;
using Platform.Delegates;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionPartialFailureTests
{
    private sealed class FaultingStore(string path) : NamedTypesDecorator<uint>(path)
    {
        public bool Fail { get; set; }
        public bool Cascade { get; set; }
        public bool FailBefore { get; set; }
        private void CheckBefore()
        {
            if (!FailBefore) return;
            FailBefore = false;
            throw new IOException("injected before mutation");
        }
        public override uint Create(IList<uint>? substitution, WriteHandler<uint>? handler)
        {
            CheckBefore();
            var result = base.Create(substitution, handler);
            if (Fail) { Fail = false; throw new IOException("injected after create"); }
            return result;
        }
        public override uint Update(IList<uint>? restriction, IList<uint>? substitution, WriteHandler<uint>? handler)
        {
            CheckBefore();
            if (Cascade)
            {
                Cascade = false;
                uint added = 0;
                base.Create(null, (before, after) =>
                {
                    added = new Link<uint>(after!).Index;
                    return handler is null ? Constants.Continue : handler(before, after);
                });
                base.Update(new[] { added }, new[] { added, added, added }, handler);
            }
            var result = base.Update(restriction, substitution, handler);
            if (Fail) { Fail = false; throw new IOException("injected after update"); }
            return result;
        }
        public override uint Delete(IList<uint>? restriction, WriteHandler<uint>? handler)
        {
            CheckBefore();
            var result = base.Delete(restriction, handler);
            if (Fail) { Fail = false; throw new IOException("injected after delete"); }
            return result;
        }
    }

    [Theory]
    [InlineData("create", false)]
    [InlineData("create", true)]
    [InlineData("update", false)]
    [InlineData("update", true)]
    [InlineData("delete", false)]
    [InlineData("delete", true)]
    [InlineData("cascade", false)]
    [InlineData("cascade", true)]
    [InlineData("cascade-success", true)]
    [InlineData("callback-create", false)]
    [InlineData("callback-create", true)]
    [InlineData("callback-update", false)]
    [InlineData("callback-update", true)]
    [InlineData("callback-delete", false)]
    [InlineData("callback-delete", true)]
    [InlineData("before-create", false)]
    [InlineData("before-create", true)]
    [InlineData("before-update", false)]
    [InlineData("before-update", true)]
    [InlineData("before-delete", false)]
    [InlineData("before-delete", true)]
    public void ObservedPartialWritesRemainReversible(string operation, bool explicitTransaction)
    {
        var directory = Path.Combine(Path.GetTempPath(), "partial-write-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        var dataPath = Path.Combine(directory, "data"); var logPath = Path.Combine(directory, "log");
        try
        {
            uint following;
            using (var data = new FaultingStore(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var tx = new TransactionsDecorator(data, log))
            {
                var id = tx.CreateAndUpdate(0u, 0u);
                using var transaction = explicitTransaction ? tx.BeginTransaction() : null;
                data.Fail = operation != "cascade-success" && !operation.StartsWith("callback-") && !operation.StartsWith("before-");
                data.FailBefore = operation.StartsWith("before-");
                data.Cascade = operation.StartsWith("cascade");
                WriteHandler<uint> throwingHandler = (_, _) => throw new IOException("injected callback");
                Action write = () =>
                {
                    switch (operation.Replace("before-", ""))
                    {
                        case "create": tx.Create(null, null); break;
                        case "callback-create": tx.Create(null, throwingHandler); break;
                        case "callback-update": tx.Update(new[] { id }, new[] { id, id, id }, throwingHandler); break;
                        case "callback-delete": tx.Delete(new[] { id }, throwingHandler); break;
                        case "cascade": case "cascade-success": case "update": tx.Update(new[] { id }, new[] { id, id, id }, null); break;
                        case "delete": tx.Delete(new[] { id }, null); break;
                    }
                };
                if (operation != "cascade-success")
                {
                    var error = Assert.Throws<IOException>(write);
                    Assert.StartsWith("injected ", error.Message);
                }
                else write();
                data.Fail = false;
                if (transaction is not null)
                {
                    Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
                    transaction.Rollback();
                }
                Assert.Equal(1u, tx.Count());
                Assert.True(tx.Exists(id)); Assert.Equal(0u, tx.GetSource(id)); Assert.Equal(0u, tx.GetTarget(id));
                using (var next = tx.BeginTransaction()) next.Rollback();
                following = tx.CreateAndUpdate(id, id);
            }
            using var reopenedData = new NamedTypesDecorator<uint>(dataPath);
            using var reopenedLog = new NamedTypesDecorator<uint>(logPath);
            using var reopened = new TransactionsDecorator(reopenedData, reopenedLog);
            Assert.Equal(2u, reopened.Count()); Assert.True(reopened.Exists(following));
            Assert.Equal(0u, reopened.GetSource(1u)); Assert.Equal(0u, reopened.GetTarget(1u));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
