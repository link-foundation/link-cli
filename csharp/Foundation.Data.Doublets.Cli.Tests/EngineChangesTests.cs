using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineChangesTests
{
    private static EngineOptions Options(StateStructure structure, RevisionEncoding encoding) => new()
    { StateStructure = structure, RevisionEncoding = encoding, HistoryLimit = null };
    [Theory]
    [InlineData(StateStructure.Flat)]
    [InlineData(StateStructure.Shared)]
    public void SelectedEncodingVisitsOnlyChangedRowsAndPreservesAllocationAndNoopSemantics(StateStructure structure)
    {
        foreach (var encoding in Enum.GetValues<RevisionEncoding>())
        {
            var log = new MemoryCommitLog(); var engine = new LinksEngine<ulong>(log, Options(structure, encoding));
            using (var seed = engine.Begin())
            { for (ulong id = 1; id <= 128; id++) { seed.EnsureCreated(id); seed.SetName(id, $"unchanged {id} {new string('雪', 16)}"); } seed.Commit(); }
            var pinned = engine.Snapshot();
            for (ulong step = 1; step <= 16; step++) { using var edit = engine.Begin(); edit.UpdateLink(1, step, 0); edit.Commit(); }
            var records = log.Read(); Assert.Equal(17, records.Count); int rows = 0;
            foreach (var record in records.Skip(1))
            {
                var fields = record.Split('|'); bool changes = encoding == RevisionEncoding.Changes;
                Assert.Equal(changes ? "engine2" : "engine1", fields[0]);
                int count = fields[changes ? 4 : 3].Split(';').Length; Assert.Equal(changes ? 1 : 128, count); rows += count;
            }
            Assert.Equal(encoding == RevisionEncoding.Changes ? 16 : 2048, rows); Assert.Equal(0UL, pinned.Rows[1].Link.Source);
            foreach (var (id, row) in pinned.Rows.Where(pair => pair.Key != 1)) Assert.Equal(row, engine.Snapshot().Rows[id]);
            using (var equal = engine.Begin()) { equal.UpdateLink(1, 16, 0); Assert.Equal(18UL, equal.Commit()); }
            using (var transient = engine.Begin()) { Assert.Equal(129UL, transient.CreateLink(0, 0)); transient.DeleteLink(129); Assert.Equal(19UL, transient.Commit()); }
            if (encoding == RevisionEncoding.Changes)
            { Assert.Equal("", log.Read()[17].Split('|')[4]); Assert.Equal("", log.Read()[18].Split('|')[4]); }
            foreach (var reopenStructure in Enum.GetValues<StateStructure>())
                foreach (var reopenEncoding in Enum.GetValues<RevisionEncoding>())
                {
                    var saved = new MemoryCommitLog(); foreach (var record in log.Read()) saved.Append(record);
                    var reopened = new LinksEngine<ulong>(saved, Options(reopenStructure, reopenEncoding));
                    Assert.Equal(engine.Snapshot().Rows, reopened.Snapshot().Rows);
                    using var following = reopened.Begin(); Assert.Equal(130UL, following.CreateLink(0, 0)); following.Commit();
                }
        }
        Assert.Throws<ArgumentException>(() => new LinksEngine<ulong>(new MemoryCommitLog(), new() { RevisionEncoding = (RevisionEncoding)99 }));
    }

    [Theory]
    [InlineData(StateStructure.Flat)]
    [InlineData(StateStructure.Shared)]
    public void GroupsAndEvictedMergeBasesReplayMixedEncodingsAndStateChoices(StateStructure structure)
    {
        foreach (var branchStructure in Enum.GetValues<StateStructure>())
            foreach (var encoding in Enum.GetValues<RevisionEncoding>())
                foreach (var branchEncoding in Enum.GetValues<RevisionEncoding>())
                {
                    var log = new MemoryCommitLog(); var options = Options(structure, encoding) with { Isolation = IsolationLevel.Snapshot };
                    var engine = new LinksEngine<ulong>(log, options);
                    using (var seed = engine.Begin()) { seed.EnsureCreated(1); seed.EnsureCreated(2); seed.Commit(); }
                    var pinned = engine.Snapshot(); using var a = engine.Begin(); using var b = engine.Begin(); a.SetName(1, "one"); b.SetName(2, "two");
                    Assert.Equal(new[] { 2UL, 3UL }, engine.CommitGroup(new[] { a, b }).Select(result => result.Revision!.Value));
                    var childLog = new MemoryCommitLog(); var childOptions = Options(branchStructure, branchEncoding) with { HistoryLimit = 0 };
                    var child = engine.Fork(childLog, childOptions); var mergeBase = child.MergeBaseRecord; Assert.StartsWith("engine1|", mergeBase);
                    using (var remote = child.Engine.Begin()) { remote.DeleteLink(2); remote.EnsureCreated(ulong.MaxValue); remote.SetName(ulong.MaxValue, ""); remote.Commit(); }
                    Assert.Throws<KeyNotFoundException>(() => child.Engine.SnapshotAt(1));
                    if (branchEncoding == RevisionEncoding.Changes) Assert.Throws<InvalidDataException>(() => engine.ResumeBranch(childLog, childLog.Read()[0], childOptions));
                    var resumed = engine.ResumeBranch(childLog, mergeBase, childOptions); Assert.Equal(4UL, resumed.Merge()); Assert.Equal(4UL, resumed.Merge());
                    Assert.Equal(2, pinned.Rows.Count); Assert.Null(pinned.Rows[1].Name);
                    var reopened = new LinksEngine<ulong>(log, options); Assert.False(reopened.Snapshot().Rows.ContainsKey(2)); Assert.Equal("", reopened.Snapshot().Rows[ulong.MaxValue].Name);
                    using var following = reopened.Begin(); Assert.Throws<OverflowException>(() => following.CreateLink(0, 0)); following.SetName(1, "following"); following.Commit();
                }
    }

    [Theory]
    [InlineData(StateStructure.Flat)]
    [InlineData(StateStructure.Shared)]
    public void ChangeEncodingPreservesAdmissionVisibilityValidationConstraintsAndHistory(StateStructure structure)
    {
        foreach (var concurrency in Enum.GetValues<ConcurrencyControl>())
            foreach (var isolation in Enum.GetValues<IsolationLevel>())
                foreach (var validation in Enum.GetValues<SerializableValidation>())
                {
                    if (isolation != IsolationLevel.Serializable && validation == SerializableValidation.ReadSet) continue;
                    var options = Options(structure, RevisionEncoding.Changes) with { Concurrency = concurrency, Isolation = isolation, SerializableValidation = validation };
                    var engine = new LinksEngine<uint>(new MemoryCommitLog(), options);
                    using (var seed = engine.Begin()) { seed.EnsureCreated(1); seed.EnsureCreated(2); seed.Commit(); }
                    using var reader = engine.Begin(); Assert.Equal(2U, reader.Count(null));
                    if (concurrency == ConcurrencyControl.Exclusive) { Assert.Throws<InvalidOperationException>(() => engine.Begin()); reader.Rollback(); continue; }
                    using (var writer = engine.Begin()) { writer.SetName(1, "changed"); writer.Commit(); }
                    Assert.Equal(isolation == IsolationLevel.ReadCommitted ? 1U : 0U, reader.GetByName("changed")); reader.SetName(2, "reader");
                    if (isolation == IsolationLevel.Serializable) Assert.Throws<InvalidOperationException>(() => reader.Commit()); else reader.Commit();
                }
        foreach (var constraint in Enum.GetValues<EngineConstraint>())
        {
            var log = new MemoryCommitLog(); var engine = new LinksEngine<uint>(log, Options(structure, RevisionEncoding.Changes) with { Constraints = new[] { constraint } });
            using var bad = engine.Begin(); bad.CreateLink(0, 0); bad.CreateLink(0, 0);
            if (constraint == EngineConstraint.References) bad.UpdateLink(1, 99, 0);
            if (constraint == EngineConstraint.UniqueNames) { bad.SetName(1, "x"); bad.SetName(2, "x"); }
            Assert.Throws<InvalidOperationException>(() => bad.Commit()); Assert.Empty(log.Read());
            using var good = engine.Begin(); good.CreateLink(0, 0); good.Commit();
        }
        foreach (int? history in new int?[] { 0, 1, null })
        {
            var log = new MemoryCommitLog(); var options = Options(structure, RevisionEncoding.Changes) with { HistoryLimit = history };
            var engine = new LinksEngine<uint>(log, options); using (var seed = engine.Begin()) { seed.CreateLink(0, 0); seed.Commit(); }
            var pinned = engine.Snapshot(); using (var edit = engine.Begin()) { edit.SetName(1, "new"); edit.Commit(); }
            Assert.Null(pinned.Rows[1].Name);
            if (history is null) Assert.Same(pinned, engine.SnapshotAt(1)); else Assert.Throws<KeyNotFoundException>(() => engine.SnapshotAt(1));
            Assert.Equal("new", new LinksEngine<uint>(log, options).Snapshot().Rows[1].Name);
        }
    }
}
