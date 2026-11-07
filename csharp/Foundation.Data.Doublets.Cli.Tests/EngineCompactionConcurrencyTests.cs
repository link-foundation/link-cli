using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineCompactionConcurrencyTests
{
    private sealed class BlockingLog : IReplaceableCommitLog, IDisposable
    {
        internal readonly ManualResetEventSlim Entered = new(), Release = new();
        internal string[] Entries = Array.Empty<string>();
        public bool SupportsDurableFlush => false;
        public IReadOnlyList<string> Read() => Entries.ToArray();
        public void Append(string entry) => Entries = Entries.Append(entry).ToArray();
        public void Flush() { }
        public bool Replace(IReadOnlyList<string> records)
        {
            Entered.Set();
            if (!Release.Wait(TimeSpan.FromSeconds(10))) throw new IOException("Publication release deadline.");
            bool changed = !Entries.SequenceEqual(records); Entries = records.ToArray(); return changed;
        }
        public void Dispose() { Entered.Dispose(); Release.Dispose(); }
    }
    [Fact]
    public async Task CompactionHoldsCommitGateContinuouslyThroughPublication()
    {
        using var log = new BlockingLog(); using var started = new ManualResetEventSlim();
        var engine = new LinksEngine<uint>(log); using var tx = engine.Begin(); tx.CreateLink(0, 0);
        var compact = Task.Run(engine.CompactJournal);
        Task<ulong>? commit = null; bool blocked = false;
        try
        {
            Assert.True(log.Entered.Wait(TimeSpan.FromSeconds(10)));
            commit = Task.Run(() => { started.Set(); return tx.Commit(); });
            Assert.True(started.Wait(TimeSpan.FromSeconds(10)));
            blocked = await Task.WhenAny(commit, Task.Delay(100)) != commit;
        }
        finally { log.Release.Set(); }
        Assert.True(await compact.WaitAsync(TimeSpan.FromSeconds(10)));
        if (commit is not null) Assert.Equal(1ul, await commit.WaitAsync(TimeSpan.FromSeconds(10)));
        Assert.True(blocked, "A commit crossed an unpublished checkpoint.");
        Assert.Equal(2, log.Entries.Length); Assert.StartsWith("engine3|0|", log.Entries[0]);
        var restored = new LinksEngine<uint>(log); Assert.Equal(1ul, restored.Snapshot().Revision); Assert.Single(restored.Snapshot().Rows);
    }
}
