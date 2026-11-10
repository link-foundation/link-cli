using System.Reflection;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineMetadataCostTests
{
    private static Dictionary<ulong, ulong>? Metadata(LinksEngine<ulong> engine) =>
        (Dictionary<ulong, ulong>?)typeof(LinksEngine<ulong>)
            .GetField("Changed", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(engine);
    private static int Count(LinksEngine<ulong> engine) => Metadata(engine)?.Count ?? 0;
    private static LinksEngine<ulong> Seed(MemoryCommitLog log, EngineOptions options)
    {
        var engine = new LinksEngine<ulong>(log, options);
        using var tx = engine.Begin();
        for (ulong id = 1; id <= 128; id++) tx.EnsureCreated(id);
        tx.Commit();
        return engine;
    }
    [Theory]
    [InlineData(ConcurrencyControl.Optimistic, IsolationLevel.Serializable)]
    [InlineData(ConcurrencyControl.Optimistic, IsolationLevel.Snapshot)]
    [InlineData(ConcurrencyControl.Optimistic, IsolationLevel.ReadCommitted)]
    [InlineData(ConcurrencyControl.Exclusive, IsolationLevel.Serializable)]
    [InlineData(ConcurrencyControl.Exclusive, IsolationLevel.Snapshot)]
    [InlineData(ConcurrencyControl.Exclusive, IsolationLevel.ReadCommitted)]
    [InlineData(ConcurrencyControl.Pessimistic, IsolationLevel.Serializable)]
    [InlineData(ConcurrencyControl.Pessimistic, IsolationLevel.Snapshot)]
    [InlineData(ConcurrencyControl.Pessimistic, IsolationLevel.ReadCommitted)]
    public void ReplayAndSingleWriterNeedNoGenerations(ConcurrencyControl concurrency, IsolationLevel isolation)
    {
        var log = new MemoryCommitLog();
        var options = new EngineOptions { Concurrency = concurrency, Isolation = isolation };
        var engine = Seed(log, options);
        Assert.Null(Metadata(engine));
        var reopened = new LinksEngine<ulong>(log, options);
        Assert.Null(Metadata(reopened));
        using var tx = reopened.Begin();
        tx.UpdateLink(1, 2, 3);
        tx.Commit();
        Assert.Equal(2ul, reopened.Snapshot().Rows[1].Link.Source);
        Assert.Equal(0, Count(reopened));
    }
    [Fact]
    public void SerializableNeedsNoRowGenerations()
    {
        var engine = Seed(new(), new());
        using var stale = engine.Begin();
        using var writer = engine.Begin();
        writer.UpdateLink(1, 2, 3); writer.Commit();
        Assert.Equal(0, Count(engine));
        stale.UpdateLink(2, 3, 4);
        Assert.Throws<InvalidOperationException>(() => stale.Commit());
        Assert.Equal(0, Count(engine));
    }
    [Theory]
    [InlineData(IsolationLevel.ReadCommitted)]
    [InlineData(IsolationLevel.Snapshot)]
    public void OptimisticGenerationsProtectAbaAndDisjointWritesThenAreReclaimed(IsolationLevel isolation)
    {
        var engine = Seed(new(), new() { Isolation = isolation });
        using var stale = engine.Begin();
        stale.UpdateLink(1, 1, 0);
        using (var deleted = engine.Begin()) { deleted.DeleteLink(1); deleted.Commit(); }
        Assert.Equal(1, Count(engine));
        using (var recreated = engine.Begin()) { recreated.EnsureCreated(1); recreated.Commit(); }
        Assert.Equal(1, Count(engine));
        Assert.Throws<InvalidOperationException>(() => stale.Commit());
        Assert.Equal(0, Count(engine));
        Assert.Equal(0ul, engine.Snapshot().Rows[1].Link.Source);
        using var first = engine.Begin();
        using var second = engine.Begin();
        first.UpdateLink(1, 1, 0); second.UpdateLink(2, 2, 0);
        first.Commit(); Assert.Equal(1, Count(engine));
        second.Commit(); Assert.Equal(0, Count(engine));
        Assert.Equal(2ul, engine.Snapshot().Rows[2].Link.Source);
        for (int finish = 0; finish < 3; finish++)
        {
            var observer = engine.Begin();
            using var writer = engine.Begin();
            writer.UpdateLink(3, 3, 0); writer.Commit();
            Assert.Equal(1, Count(engine));
            if (finish == 0) observer.Commit();
            else if (finish == 1) observer.Rollback();
            observer.Dispose();
            Assert.Equal(0, Count(engine));
        }
    }
    [Fact]
    public void BranchPublicationsStillConflictWithStaleSessions()
    {
        var engine = Seed(new(), new() { Isolation = IsolationLevel.Snapshot });
        var branch = engine.Fork(new MemoryCommitLog());
        using var stale = engine.Begin();
        stale.UpdateLink(1, 1, 0);
        using (var incoming = branch.Engine.Begin()) { incoming.UpdateLink(1, 2, 0); incoming.Commit(); }
        branch.Merge();
        Assert.Equal(1, Count(engine));
        Assert.Throws<InvalidOperationException>(() => stale.Commit());
        Assert.Equal(0, Count(engine));
        Assert.Equal(2ul, engine.Snapshot().Rows[1].Link.Source);
    }
    [Fact]
    public void ReadSetsAreAllocatedOnlyWhenSelectedAndReleasedOnFinish()
    {
        var field = typeof(EngineSession<ulong>).GetField("_reads", BindingFlags.Instance | BindingFlags.NonPublic)!;
        foreach (var concurrency in new[] { ConcurrencyControl.Optimistic, ConcurrencyControl.Exclusive, ConcurrencyControl.Pessimistic })
            foreach (var validation in new[] { SerializableValidation.Revision, SerializableValidation.ReadSet })
            {
                bool selected = concurrency != ConcurrencyControl.Exclusive && validation == SerializableValidation.ReadSet;
                var engine = Seed(new(), new() { Concurrency = concurrency, SerializableValidation = validation });
                using var reader = engine.Begin(); reader.GetLink(1);
                Assert.Equal(selected, field.GetValue(reader) is not null);
                if (selected)
                {
                    Assert.Single((HashSet<ulong>)field.GetValue(reader)!);
                    using var writer = engine.Begin(); writer.UpdateLink(2, 2, 0); writer.Commit();
                    Assert.Equal(1, Count(engine));
                }
                reader.Commit(); Assert.Null(field.GetValue(reader)); Assert.Equal(0, Count(engine));
                reader.GetLink(1); reader.Count(null); Assert.Null(field.GetValue(reader));
                using var discarded = engine.Begin(); discarded.GetLink(1); discarded.Rollback();
                Assert.Null(field.GetValue(discarded));
            }
    }

}
