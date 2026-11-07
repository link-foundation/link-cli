using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>Savepoints restore staged rows without weakening isolation or reusing addresses.</summary>
public class EngineSavepointTests
{
    private static LinksEngine<uint> Seeded(EngineOptions? options = null)
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog(), options ?? new EngineOptions());
        using var seed = engine.Begin();
        seed.CreateLink(0, 0); seed.CreateLink(0, 0); seed.SetName(1, "kept"); seed.Commit();
        return engine;
    }

    [Fact]
    public void RollbackRestoresCreatedUpdatedDeletedAndRenamedRows()
    {
        var engine = Seeded();
        using var session = engine.Begin();
        var staged = session.CreateLink(1, 1);
        var savepoint = session.Savepoint();
        session.UpdateLink(1, 2, 2); session.SetName(1, "changed"); session.DeleteLink(2);
        session.UpdateLink(staged, 2, 2);
        var dropped = session.CreateLink(0, 0);

        session.RollbackTo(savepoint);

        Assert.Equal(0u, session.GetLink(1)!.Value.Source);
        Assert.Equal("kept", session.GetName(1));
        Assert.Equal(0u, session.GetByName("changed"));
        Assert.NotNull(session.GetLink(2));
        Assert.Equal(1u, session.GetLink(staged)!.Value.Source);
        Assert.Null(session.GetLink(dropped));

        // Writes after the rollback stay and the discarded address is not reused.
        var next = session.CreateLink(0, 0);
        Assert.Equal(dropped + 1, next);
        session.Commit();
        Assert.True(engine.Current.Rows.TryGetValue(staged, out _));
        Assert.True(engine.Current.Rows.TryGetValue(next, out _));
        Assert.False(engine.Current.Rows.TryGetValue(dropped, out _));
    }

    [Fact]
    public void SavepointsNestStayUsableAndReleaseKeepsWrites()
    {
        var engine = Seeded();
        using var session = engine.Begin();
        var outer = session.Savepoint();
        session.UpdateLink(1, 1, 1);
        var middle = session.Savepoint();
        session.UpdateLink(1, 2, 2);
        var inner = session.Savepoint();
        session.UpdateLink(1, 1, 2);

        session.ReleaseSavepoint(middle);
        Assert.Equal(2u, session.GetLink(1)!.Value.Target);
        foreach (var gone in new[] { middle, inner })
        {
            Assert.Contains("Unknown savepoint", Assert.Throws<InvalidOperationException>(() => session.RollbackTo(gone)).Message);
            Assert.Contains("Unknown savepoint", Assert.Throws<InvalidOperationException>(() => session.ReleaseSavepoint(gone)).Message);
        }

        session.RollbackTo(outer);
        Assert.Equal(0u, session.GetLink(1)!.Value.Source);
        var later = session.Savepoint();
        session.UpdateLink(2, 1, 1);
        session.RollbackTo(outer);
        Assert.Equal(0u, session.GetLink(2)!.Value.Source);
        Assert.Throws<InvalidOperationException>(() => session.RollbackTo(later));
    }

    [Fact]
    public void TokensOfOtherSessionsAndClosedSessionsAreRefused()
    {
        var engine = Seeded();
        using var first = engine.Begin();
        using var second = engine.Begin();
        var foreign = first.Savepoint();
        second.UpdateLink(1, 1, 1);
        Assert.Throws<InvalidOperationException>(() => second.RollbackTo(foreign));
        Assert.Throws<InvalidOperationException>(() => second.ReleaseSavepoint(default));
        Assert.Equal(1u, second.GetLink(1)!.Value.Source);

        var own = second.Savepoint();
        second.Commit();
        Assert.Contains("closed", Assert.Throws<InvalidOperationException>(() => second.RollbackTo(own)).Message);
        Assert.Contains("closed", Assert.Throws<InvalidOperationException>(() => second.ReleaseSavepoint(own)).Message);
        Assert.Contains("closed", Assert.Throws<InvalidOperationException>(() => second.Savepoint()).Message);
        first.Rollback();
        Assert.Throws<InvalidOperationException>(() => first.RollbackTo(foreign));
    }

    [Fact]
    public void ASessionRolledBackToNoWritesPublishesNoRevision()
    {
        var engine = Seeded();
        var before = engine.Current.Revision;
        using var session = engine.Begin();
        var savepoint = session.Savepoint();
        session.UpdateLink(1, 1, 1);
        session.RollbackTo(savepoint);
        Assert.Equal(before, session.Commit());
        Assert.Equal(before, engine.Current.Revision);
    }

    [Fact]
    public void DiscardedWritesDoNotConflictButRetainedWritesDo()
    {
        var engine = Seeded(new EngineOptions { Isolation = IsolationLevel.Snapshot });
        using (var session = engine.Begin())
        {
            session.UpdateLink(2, 1, 1);
            var savepoint = session.Savepoint();
            session.UpdateLink(1, 1, 1);
            session.RollbackTo(savepoint);
            using (var other = engine.Begin()) { other.UpdateLink(1, 2, 2); other.Commit(); }
            session.Commit();
        }
        using var retained = engine.Begin();
        retained.UpdateLink(2, 2, 2);
        retained.RollbackTo(retained.Savepoint());
        using (var other = engine.Begin()) { other.UpdateLink(2, 0, 0); other.Commit(); }
        Assert.Contains("Conflict", Assert.Throws<InvalidOperationException>(() => retained.Commit()).Message);
    }

    [Fact]
    public void ReadsBeforeARollbackStayValidated()
    {
        var engine = Seeded(new EngineOptions { Isolation = IsolationLevel.Serializable, SerializableValidation = SerializableValidation.ReadSet });
        using var session = engine.Begin();
        var savepoint = session.Savepoint();
        var observed = session.GetLink(1)!.Value;
        session.UpdateLink(2, observed.Source, observed.Target);
        session.RollbackTo(savepoint);
        session.UpdateLink(2, 1, 1);
        using (var other = engine.Begin()) { other.UpdateLink(1, 1, 1); other.Commit(); }
        Assert.Contains("read link", Assert.Throws<InvalidOperationException>(() => session.Commit()).Message);
    }

    [Fact]
    public void PointWriteOwnershipLastsUntilTheSessionEnds()
    {
        var engine = Seeded(new EngineOptions { Concurrency = ConcurrencyControl.Pessimistic });
        using var session = engine.Begin();
        var savepoint = session.Savepoint();
        session.UpdateLink(1, 1, 1);
        session.RollbackTo(savepoint);
        using var other = engine.Begin();
        Assert.ThrowsAny<InvalidOperationException>(() => other.UpdateLink(1, 2, 2));
        session.Rollback();
        other.UpdateLink(1, 2, 2);
        other.Commit();
    }

    [Fact]
    public void ReadCommittedSeesCommitsMadeAfterTheSavepoint()
    {
        var engine = Seeded(new EngineOptions { Isolation = IsolationLevel.ReadCommitted });
        using var session = engine.Begin();
        var savepoint = session.Savepoint();
        session.UpdateLink(1, 1, 1);
        using (var other = engine.Begin()) { other.UpdateLink(1, 2, 2); other.Commit(); }
        Assert.Equal(1u, session.GetLink(1)!.Value.Source);
        session.RollbackTo(savepoint);
        Assert.Equal(2u, session.GetLink(1)!.Value.Source);
    }

    [Fact]
    public void ShardSavepointsCoverOneShardAndCloseWithTheGroup()
    {
        var engine = new ClusterEngine<uint>(new EngineClusterTests.SavepointStore());
        var group = engine.Begin("a", "b");
        group.Shard("a").CreateLink(0, 0);
        var b = group.Shard("b");
        var savepoint = b.Savepoint();
        var inner = b.Savepoint();
        b.CreateLink(0, 0);
        b.ReleaseSavepoint(inner);
        b.RollbackTo(savepoint);
        Assert.NotNull(group.Shard("a").GetLink(1));
        Assert.Null(b.GetLink(1));
        group.Commit();
        Assert.Throws<InvalidOperationException>(() => b.Savepoint());
        Assert.Throws<InvalidOperationException>(() => b.RollbackTo(savepoint));
        Assert.Throws<InvalidOperationException>(() => b.ReleaseSavepoint(savepoint));
        using var check = engine.Begin("a", "b");
        Assert.NotNull(check.Shard("a").GetLink(1));
        Assert.Null(check.Shard("b").GetLink(1));
    }

    [Fact]
    public void ReservedAddressesStayConsumedAfterRollback()
    {
        using var session = Seeded().Begin();
        session.ReserveQueryAllocations(preview => preview.Create(null, null));
        var savepoint = session.Savepoint();
        Assert.Equal(3u, session.CreateLink(0, 0));
        session.RollbackTo(savepoint);
        Assert.Equal(4u, session.CreateLink(0, 0));
    }
}
