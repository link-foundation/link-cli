using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineCheckpointTests
{
    [Fact]
    public void IndependentCheckpointVectorsReplayOrReject()
    {
        using var vectors = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "engine-checkpoint-vectors.json")));
        foreach (string kind in new[] { "valid", "invalidU32" })
        {
            foreach (var item in vectors.RootElement.GetProperty(kind).EnumerateArray())
            {
                var log = new MemoryCommitLog();
                foreach (var record in item.EnumerateArray()) log.Append(record.GetString()!);
                if (kind == "valid")
                {
                    var engine = new LinksEngine<uint>(log);
                    if (log.Read().Count == 1) Assert.Equal(log.Read(), engine.ExportCheckpoint());
                }
                else Assert.Throws<InvalidDataException>(() => new LinksEngine<uint>(log));
            }
        }
    }

    [Fact]
    public void CheckpointsPreserveSelectedHistoryNamesHolesAndFollowingChanges()
    {
        foreach (var structure in Enum.GetValues<StateStructure>())
            foreach (var encoding in Enum.GetValues<RevisionEncoding>())
                foreach (int? history in new int?[] { 0, 2, null })
                {
                    var options = new EngineOptions { StateStructure = structure, RevisionEncoding = encoding, HistoryLimit = history };
                    var log = new MemoryCommitLog(); var engine = new LinksEngine<uint>(log, options);
                    for (int revision = 1; revision <= 6; revision++)
                    {
                        using var tx = engine.Begin();
                        if (revision == 1) tx.CreateLink(0, 0);
                        if (revision == 3) { tx.EnsureCreated(7); tx.DeleteLink(7); }
                        tx.SetName(1, $"雪 {revision}"); tx.Commit();
                    }
                    var original = log.Read().ToArray(); var exported = engine.ExportCheckpoint();
                    Assert.StartsWith("engine3|", exported[0]);
                    Assert.Equal(history == 0 ? 1 : history ?? 7, exported.Count);
                    Assert.Equal(original, log.Read());
                    var copyLog = new MemoryCommitLog(); foreach (var entry in exported) copyLog.Append(entry);
                    var copy = new LinksEngine<uint>(copyLog, options);
                    for (ulong revision = 0; revision <= 6; revision++)
                    {
                        bool retained = history is null || revision == 6 || (history == 2 && revision == 5);
                        if (retained) Assert.Equal(engine.SnapshotAt(revision).Rows.ToArray(), copy.SnapshotAt(revision).Rows.ToArray());
                        else Assert.Throws<KeyNotFoundException>(() => copy.SnapshotAt(revision));
                    }
                    using var write = copy.Begin(); Assert.Equal(8u, write.CreateLink(1, 0)); write.Commit();
                    var reopened = new LinksEngine<uint>(copyLog);
                    Assert.Equal(7ul, reopened.Snapshot().Revision);
                    Assert.Equal("雪 6", reopened.Snapshot().Rows[1].Name);
                    Assert.True(reopened.Snapshot().Rows.ContainsKey(8));
                }
    }

    [Fact]
    public void ExportExcludesPendingReservationsAndRejectsUncertainOwner()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        LinksEngine<uint> Copy()
        {
            var log = new MemoryCommitLog(); foreach (var entry in engine.ExportCheckpoint()) log.Append(entry);
            return new(log);
        }
        using var pending = engine.Begin(); pending.EnsureCreated(99);
        using (var copy = Copy().Begin()) Assert.Equal(1u, copy.CreateLink(0, 0));
        pending.Rollback();
        using (var write = engine.Begin()) { Assert.Equal(100u, write.CreateLink(0, 0)); write.Commit(); }
        using var second = engine.Begin(); second.EnsureCreated(199);
        using (var copy = Copy().Begin()) Assert.Equal(101u, copy.CreateLink(0, 0));
        Assert.Equal(1ul, engine.Snapshot().Revision); second.Rollback();
        Assert.Throws<IOException>(() => engine.WithLog<int>(_ => throw new IOException("unknown")));
        Assert.Throws<InvalidOperationException>(() => engine.ExportCheckpoint());
    }

    [Fact]
    public void CheckpointReplayEnforcesEachSelectedConstraint()
    {
        using var vectors = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "engine-checkpoint-vectors.json")));
        foreach (var structure in Enum.GetValues<StateStructure>())
            foreach (var validation in Enum.GetValues<ConstraintValidation>())
                foreach (var (key, constraint) in new[] { ("references", EngineConstraint.References), ("uniquePairs", EngineConstraint.UniquePairs), ("uniqueNames", EngineConstraint.UniqueNames) })
                {
                    var log = new MemoryCommitLog(); foreach (var record in vectors.RootElement.GetProperty("constraintViolations").GetProperty(key).EnumerateArray()) log.Append(record.GetString()!);
                    _ = new LinksEngine<uint>(log);
                    Assert.Throws<InvalidOperationException>(() => new LinksEngine<uint>(log, new() { StateStructure = structure, ConstraintValidation = validation, Constraints = new[] { constraint } }));
                }
    }

    [Fact]
    public void CheckpointedBranchesResumeOnlyWithRetainedExactMergeBases()
    {
        foreach (int? history in new int?[] { null, 2 })
        {
            var parent = new LinksEngine<uint>(new MemoryCommitLog());
            using (var seed = parent.Begin()) { seed.CreateLink(0, 0); seed.Commit(); }
            var options = new EngineOptions { HistoryLimit = history };
            var branch = parent.Fork(new MemoryCommitLog(), options); var descriptor = branch.MergeBaseRecord;
            using (var edit = branch.Engine.Begin()) { edit.EnsureCreated(7); edit.DeleteLink(7); edit.SetName(1, "checkpointed branch 雪"); edit.Commit(); }
            var records = branch.Engine.ExportCheckpoint(); var log = new MemoryCommitLog(); foreach (string entry in records) log.Append(entry);
            var pruned = new LinksEngine<uint>(log); var prunedLog = new MemoryCommitLog(); foreach (string entry in pruned.ExportCheckpoint()) prunedLog.Append(entry);
            Assert.Throws<InvalidDataException>(() => parent.ResumeBranch(prunedLog, descriptor, options));
            var resumed = parent.ResumeBranch(log, descriptor, options); Assert.Equal(2ul, resumed.Merge());
            Assert.Equal("checkpointed branch 雪", parent.Snapshot().Rows[1].Name);
            using var following = parent.Begin(); Assert.Equal(8u, following.CreateLink(0, 0)); following.Commit();
        }
    }
}
