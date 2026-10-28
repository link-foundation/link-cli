using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineBranchTests
{
    [Fact]
    public void ThreeWayMergePreservesIndependentWritesAndRejectsOverlappingEdits()
    {
        var parent = new LinksEngine<uint>(new MemoryCommitLog());
        using (var seed = parent.Begin()) { seed.EnsureCreated(1); seed.EnsureCreated(2); seed.Commit(); }
        var branch = parent.Fork(new MemoryCommitLog());
        using (var local = parent.Begin()) { local.UpdateLink(1, 1, 0); local.Commit(); }
        using (var remote = branch.Engine.Begin()) { remote.SetName(2, "branch"); remote.Commit(); }
        Assert.Null(parent.Snapshot().Rows[2].Name);
        Assert.Equal(3ul, branch.Merge());
        Assert.Equal(1u, parent.Snapshot().Rows[1].Link.Source);
        Assert.Equal("branch", parent.Snapshot().Rows[2].Name);
        Assert.Equal(3ul, branch.Merge());
        using (var remote = branch.Engine.Begin()) { remote.SetName(2, "again"); remote.Commit(); }
        Assert.Equal(4ul, branch.Merge());
        using (var local = parent.Begin()) { local.DeleteLink(2); local.Commit(); }
        using (var remote = branch.Engine.Begin()) { remote.SetName(2, "conflict"); remote.Commit(); }
        Assert.Throws<InvalidOperationException>(() => branch.Merge());
        Assert.False(parent.Snapshot().Rows.ContainsKey(2));
    }

    [Fact]
    public void SavedMergeBaseSupportsReopeningAfterHistoryEviction()
    {
        var parent = new LinksEngine<uint>(new MemoryCommitLog());
        using (var seed = parent.Begin()) { seed.CreateLink(0, 0); seed.Commit(); }
        var log = new MemoryCommitLog();
        var branch = parent.Fork(log, new() { HistoryLimit = 1 });
        var descriptor = branch.MergeBaseRecord;
        using (var remote = branch.Engine.Begin()) { remote.DeleteLink(1); remote.Commit(); }
        var resumed = parent.ResumeBranch(log, descriptor);
        Assert.Equal(2ul, resumed.Merge());
        Assert.Empty(parent.Snapshot().Rows);
        using var tx = parent.Begin();
        Assert.Equal(2u, tx.CreateLink(0, 0));
        Assert.Throws<ArgumentException>(() => parent.Fork(log));
        Assert.Throws<InvalidDataException>(() => parent.ResumeBranch(new MemoryCommitLog(), descriptor));
        Assert.Throws<InvalidDataException>(() => parent.ResumeBranch(log, "invalid"));
    }

    [Fact]
    public void MergeHonorsAdmissionConstraintsAndPrivateSessions()
    {
        var parent = new LinksEngine<uint>(new MemoryCommitLog(), new()
        {
            Concurrency = ConcurrencyControl.Exclusive,
            Constraints = new[] { EngineConstraint.References }
        });
        using (var seed = parent.Begin()) { seed.CreateLink(0, 0); seed.Commit(); }
        var branch = parent.Fork(new MemoryCommitLog());
        using (var remote = branch.Engine.Begin()) { remote.UpdateLink(1, 999, 0); remote.Commit(); }
        using (var held = parent.Begin()) Assert.Throws<InvalidOperationException>(() => branch.Merge());
        Assert.Throws<InvalidOperationException>(() => branch.Merge());
        Assert.Equal(0u, parent.Snapshot().Rows[1].Link.Source);
        using (var remote = branch.Engine.Begin()) { remote.UpdateLink(1, 1, 0); remote.Commit(); }
        Assert.Equal(2ul, branch.Merge());
        using var unfinished = branch.Engine.Begin(); unfinished.CreateLink(0, 0);
        Assert.Equal(2ul, branch.Merge());
        Assert.Single(parent.Snapshot().Rows);
    }

    [Fact]
    public void FailedMergeRequiresReopenAndRecoveryDoesNotDuplicateIt()
    {
        var log = new FaultLog(); var branchLog = new MemoryCommitLog();
        var parent = new LinksEngine<uint>(log);
        var branch = parent.Fork(branchLog);
        var descriptor = branch.MergeBaseRecord;
        using (var remote = branch.Engine.Begin()) { remote.CreateLink(0, 0); remote.Commit(); }
        log.Fail = true;
        Assert.Throws<IOException>(() => branch.Merge());
        Assert.Throws<InvalidOperationException>(() => parent.Snapshot());
        Assert.Throws<InvalidOperationException>(() => parent.Fork(new MemoryCommitLog()));
        Assert.Throws<InvalidOperationException>(() => parent.ResumeBranch(branchLog, descriptor));
        log.Fail = false;
        var reopened = new LinksEngine<uint>(log);
        Assert.Single(reopened.Snapshot().Rows);
        var resumed = reopened.ResumeBranch(branchLog, descriptor);
        Assert.Equal(1ul, resumed.Merge());
        Assert.Throws<IOException>(() => reopened.Fork(new FaultLog { Fail = true }));
        Assert.Equal(1ul, reopened.Snapshot().Revision);
    }

    [Fact]
    public void AllocatorOnlyMergeIsJournaledAndUpdatesExistingConflictGenerations()
    {
        var log = new MemoryCommitLog();
        var parent = new LinksEngine<uint>(log, new() { Isolation = IsolationLevel.Snapshot });
        var branch = parent.Fork(new MemoryCommitLog());
        using (var remote = branch.Engine.Begin()) { var id = remote.CreateLink(0, 0); remote.DeleteLink(id); remote.Commit(); }
        Assert.Equal(1ul, branch.Merge()); Assert.Empty(parent.Snapshot().Rows);
        var reopened = new LinksEngine<uint>(log);
        using (var next = reopened.Begin()) Assert.Equal(2u, next.CreateLink(0, 0));
        using var old = parent.Begin(); old.EnsureCreated(2);
        using (var remote = branch.Engine.Begin()) { remote.CreateLink(1, 1); remote.Commit(); }
        branch.Merge();
        Assert.Throws<InvalidOperationException>(() => old.Commit());
    }

    private sealed class FaultLog : ICommitLog
    {
        private readonly MemoryCommitLog _log = new();
        public bool Fail;
        public bool SupportsDurableFlush => false;
        public IReadOnlyList<string> Read() => _log.Read();
        public void Append(string record) => _log.Append(record);
        public void Flush() { if (Fail) throw new IOException("injected flush failure"); }
    }
}
