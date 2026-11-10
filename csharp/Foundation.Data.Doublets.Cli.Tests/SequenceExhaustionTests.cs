using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class SequenceExhaustionTests
{
    [Theory]
    [InlineData(false, false)]
    [InlineData(true, false)]
    [InlineData(false, true)]
    [InlineData(true, true)]
    public void SequenceExhaustionPreservesUndoAndPersistentState(bool explicitTransaction, bool cascade)
    {
        var directory = Directory.CreateTempSubdirectory("sequence-exhaustion-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        uint a, b, c;
        // The C# usages resolver first dispatches a nested update and delete, then
        // reports the final two-row delete batch through the outer call.
        var boundary = long.MaxValue - (cascade ? 3 : 0);
        try
        {
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var tx = new TransactionsDecorator(data, log, trace: false))
            {
                a = tx.CreateAndUpdate(0u, 0u);
                b = tx.CreateAndUpdate(a, a);
                c = tx.CreateAndUpdate(b, b);
                var id = Guid.NewGuid();
                var state = new Link<uint>(a, 0u, 0u);
                var filler = new Transition<uint>(id, boundary, DateTimeOffset.UnixEpoch,
                    TransitionKind.Update, state, state);
                foreach (var entry in new[] { "__transactions:transition:" + filler.Serialize(),
                    "__transactions:commit:" + id.ToString("N"), "__transactions:applied:" + boundary })
                    log.SetName(log.CreateAndUpdate(0u, 0u), entry);
                data.Flush(); log.Flush();
            }
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var tx = new TransactionsDecorator(data, log, trace: false))
            {
                Assert.Equal(boundary, tx.LastLoggedSequence);
                var logCount = tx.Log.Count;
                using var transaction = explicitTransaction ? tx.BeginTransaction() : null;
                Assert.Throws<InvalidOperationException>(() =>
                {
                    if (cascade) tx.Delete(new[] { b }, null); else tx.Create(null, null);
                });
                if (explicitTransaction)
                {
                    if (cascade)
                    {
                        Assert.Throws<InvalidOperationException>(() => transaction!.Commit());
                        Assert.Throws<InvalidOperationException>(() => tx.Create(null, null));
                    }
                    transaction!.Rollback();
                }
                Assert.Equal(3u, tx.Count());
                Assert.Equal(0u, tx.GetSource(a)); Assert.Equal(0u, tx.GetTarget(a));
                Assert.Equal(a, tx.GetSource(b)); Assert.Equal(a, tx.GetTarget(b));
                Assert.Equal(b, tx.GetSource(c)); Assert.Equal(b, tx.GetTarget(c));
                var recordedBoundary = boundary + (cascade ? 2 : 0);
                Assert.Equal(recordedBoundary, tx.LastLoggedSequence);
                Assert.Equal(logCount + (cascade ? 2 : 0), tx.Log.Count);
                Assert.All(tx.Log, record => Assert.InRange(record.Sequence, 1, recordedBoundary));
                if (cascade)
                {
                    tx.Create(null, null);
                    Assert.Equal(long.MaxValue, tx.LastLoggedSequence);
                }
            }
            using var reopenedData = new NamedTypesDecorator<uint>(dataPath);
            using var reopenedLog = new NamedTypesDecorator<uint>(logPath);
            using var reopened = new TransactionsDecorator(reopenedData, reopenedLog);
            Assert.Equal(cascade ? 4u : 3u, reopened.Count());
            Assert.Equal(a, reopened.GetSource(b)); Assert.Equal(b, reopened.GetSource(c));
            Assert.Throws<InvalidOperationException>(() => reopened.Create(null, null));
            using var next = reopened.BeginTransaction();
            next.Rollback();
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
