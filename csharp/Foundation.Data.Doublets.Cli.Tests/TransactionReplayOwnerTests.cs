using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class TransactionReplayOwnerTests
{
    [Theory]
    [InlineData(false, "pending")]
    [InlineData(true, "pending")]
    [InlineData(false, "commit")]
    [InlineData(true, "commit")]
    [InlineData(false, "rollback")]
    [InlineData(true, "rollback")]
    public void UnloggedReplayWaitsForLiveOwner(bool revert, string phase)
    {
        var directory = Directory.CreateTempSubdirectory("replay-owner-").FullName;
        try
        {
            using var data = new NamedTypesDecorator<uint>(Path.Combine(directory, "data.links"));
            using var log = new NamedTypesDecorator<uint>(Path.Combine(directory, "log.links"));
            var fail = false;
            using var tx = new TransactionsDecorator(data, log, flushData: () =>
            {
                if (fail) { fail = false; throw new IOException("injected checkpoint"); }
                data.Flush();
            });
            var id = tx.CreateAndUpdate(11u, 12u);
            tx.Update(new[] { id }, new[] { id, 21u, 22u }, null);
            var previous = tx.Log.Last();
            using var pending = tx.BeginTransaction();
            tx.Update(new[] { id }, new[] { id, 31u, 32u }, null);
            if (phase != "pending")
            {
                fail = true;
                Action attempt = phase == "commit" ? pending.Commit : pending.Rollback;
                Assert.Contains("injected checkpoint", Assert.Throws<IOException>(attempt).Message);
                Assert.Equal(phase == "commit", pending.IsCommitted);
            }
            var sequence = tx.LastLoggedSequence;
            var source = tx.GetSource(id);
            Assert.Throws<InvalidOperationException>(() =>
            {
                if (revert) tx.RevertTransition(previous); else tx.ApplyTransition(previous);
            });
            Assert.Equal(source, tx.GetSource(id));
            Assert.Equal(sequence, tx.LastLoggedSequence);
            Assert.Throws<InvalidOperationException>(() => tx.BeginTransaction());
            if (phase == "commit") pending.Commit(); else pending.Rollback();
            Assert.Equal(phase == "commit" ? 31u : 21u, tx.GetSource(id));
            tx.RevertTransition(previous); Assert.Equal(11u, tx.GetSource(id));
            tx.ApplyTransition(previous); Assert.Equal(21u, tx.GetSource(id));
            Assert.Equal(sequence, tx.LastLoggedSequence);
            tx.Update(new[] { id }, new[] { id, 41u, 42u }, null);
            Assert.Equal(41u, tx.GetSource(id));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
