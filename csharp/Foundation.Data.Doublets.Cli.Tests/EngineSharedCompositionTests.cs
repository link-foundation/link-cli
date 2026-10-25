using System.Collections.Immutable;
using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineSharedCompositionTests
{
    [Theory]
    [InlineData(StateStructure.Flat)]
    [InlineData(StateStructure.Shared)]
    public void StructureChoiceIsIndependentOfAdmissionIsolationAndValidation(StateStructure structure)
    {
        foreach (var concurrency in Enum.GetValues<ConcurrencyControl>())
            foreach (var isolation in Enum.GetValues<IsolationLevel>())
                foreach (var validation in Enum.GetValues<SerializableValidation>())
                {
                    if (isolation != IsolationLevel.Serializable && validation == SerializableValidation.ReadSet) continue;
                    var engine = new LinksEngine<ulong>(new MemoryCommitLog(), new()
                    { StateStructure = structure, Concurrency = concurrency, Isolation = isolation, SerializableValidation = validation });
                    using (var seed = engine.Begin()) { seed.EnsureCreated(1); seed.EnsureCreated(2); seed.Commit(); }
                    using var reader = engine.Begin(); Assert.Equal(2UL, reader.Count(null));
                    if (concurrency == ConcurrencyControl.Exclusive)
                    { Assert.Throws<InvalidOperationException>(() => engine.Begin()); reader.Rollback(); continue; }
                    using (var writer = engine.Begin()) { writer.SetName(1, "changed"); writer.Commit(); }
                    Assert.Equal(isolation == IsolationLevel.ReadCommitted ? 1UL : 0UL, reader.GetByName("changed"));
                    reader.SetName(2, "reader");
                    if (isolation == IsolationLevel.Serializable) Assert.Throws<InvalidOperationException>(() => reader.Commit());
                    else reader.Commit();
                    Assert.Equal(isolation == IsolationLevel.Serializable ? null : "changed", reader.GetName(1));
                }
    }

    [Theory]
    [InlineData(StateStructure.Flat, StateStructure.Flat)]
    [InlineData(StateStructure.Flat, StateStructure.Shared)]
    [InlineData(StateStructure.Shared, StateStructure.Flat)]
    [InlineData(StateStructure.Shared, StateStructure.Shared)]
    public void GroupsBranchesAndReopeningPreserveMixedChoices(StateStructure parentStructure, StateStructure branchStructure)
    {
        var log = new MemoryCommitLog(); var options = new EngineOptions { StateStructure = parentStructure, Isolation = IsolationLevel.Snapshot, HistoryLimit = null };
        var engine = new LinksEngine<ulong>(log, options);
        using (var seed = engine.Begin()) { seed.EnsureCreated(1); seed.EnsureCreated(2); seed.Commit(); }
        var pinned = engine.Snapshot(); using var a = engine.Begin(); using var b = engine.Begin();
        a.SetName(1, "one"); b.SetName(2, "two");
        Assert.Equal(new[] { 2UL, 3UL }, engine.CommitGroup(new[] { a, b }).Select(result => result.Revision!.Value));
        Assert.Null(pinned.Rows[1].Name);
        var branchLog = new MemoryCommitLog(); var branchOptions = new EngineOptions { StateStructure = branchStructure };
        var branch = engine.Fork(branchLog, branchOptions); var mergeBase = branch.MergeBaseRecord;
        using (var remote = branch.Engine.Begin()) { remote.DeleteLink(2); remote.EnsureCreated(ulong.MaxValue); remote.Commit(); }
        var resumed = engine.ResumeBranch(branchLog, mergeBase, branchOptions);
        Assert.Equal(4UL, resumed.Merge()); Assert.Equal(4UL, resumed.Merge());
        var current = engine.Snapshot(); Assert.False(current.Rows.ContainsKey(2)); Assert.True(current.Rows.ContainsKey(ulong.MaxValue));
        Assert.Equal(parentStructure == StateStructure.Shared, current.Rows is ImmutableSortedDictionary<ulong, EngineRow<ulong>>);
        Assert.Equal(branchStructure == StateStructure.Shared, resumed.Engine.Snapshot().Rows is ImmutableSortedDictionary<ulong, EngineRow<ulong>>);
        using (var discarded = engine.Begin()) { discarded.SetName(1, "discarded"); discarded.Rollback(); }
        var reopened = new LinksEngine<ulong>(log, options); Assert.Equal(current.Rows, reopened.Snapshot().Rows);
        Assert.Equal(2, reopened.SnapshotAt(1).Rows.Count);
        using var following = reopened.Begin(); following.SetName(1, "following"); following.Commit();
        Assert.Equal("one", current.Rows[1].Name);
    }

    [Theory]
    [InlineData(StateStructure.Flat)]
    [InlineData(StateStructure.Shared)]
    public void ConstraintsAndRetentionPreservePriorSnapshots(StateStructure structure)
    {
        foreach (var constraint in Enum.GetValues<EngineConstraint>())
        {
            var log = new MemoryCommitLog(); var engine = new LinksEngine<ulong>(log, new() { StateStructure = structure, Constraints = new[] { constraint } });
            var before = engine.Snapshot(); using var tx = engine.Begin(); tx.CreateLink(0, 0); tx.CreateLink(0, 0);
            if (constraint == EngineConstraint.References) tx.UpdateLink(1, 99, 0);
            if (constraint == EngineConstraint.UniqueNames) { tx.SetName(1, "duplicate"); tx.SetName(2, "duplicate"); }
            Assert.Throws<InvalidOperationException>(() => tx.Commit()); Assert.Empty(before.Rows); Assert.Same(before, engine.Snapshot()); Assert.Empty(log.Read());
            using var following = engine.Begin(); following.CreateLink(0, 0); following.Commit(); Assert.Single(engine.Snapshot().Rows);
        }
        foreach (int? limit in new int?[] { 0, 1, null })
        {
            var engine = new LinksEngine<ulong>(new MemoryCommitLog(), new() { StateStructure = structure, HistoryLimit = limit });
            using (var seed = engine.Begin()) { seed.CreateLink(0, 0); seed.Commit(); }
            var pinned = engine.Snapshot(); using (var writer = engine.Begin()) { writer.SetName(1, "changed"); writer.Commit(); }
            Assert.Null(pinned.Rows[1].Name);
            if (limit is not null) Assert.Throws<KeyNotFoundException>(() => engine.SnapshotAt(1));
            else Assert.Same(pinned, engine.SnapshotAt(1));
            var current = engine.Snapshot(); using (var reader = engine.Begin()) reader.Commit();
            Assert.Same(current, engine.Snapshot());
        }
    }

    private sealed class FailingLog : ICommitLog
    {
        internal MemoryCommitLog Inner = new();
        internal bool FailAppend, FailFlush;
        public bool SupportsDurableFlush => false;
        public IReadOnlyList<string> Read() => Inner.Read();
        public void Append(string record) { if (FailAppend) throw new IOException("append"); Inner.Append(record); }
        public void Flush() { if (FailFlush) throw new IOException("flush"); }
    }
    [Theory]
    [InlineData(StateStructure.Flat, false)]
    [InlineData(StateStructure.Shared, false)]
    [InlineData(StateStructure.Flat, true)]
    [InlineData(StateStructure.Shared, true)]
    public void UnknownCommitPreservesPinnedSnapshotAndReopensTheActualOutcome(StateStructure structure, bool flush)
    {
        var log = new FailingLog(); var options = new EngineOptions { StateStructure = structure };
        var engine = new LinksEngine<ulong>(log, options); using (var seed = engine.Begin()) { seed.CreateLink(0, 0); seed.Commit(); }
        var before = engine.Snapshot(); log.FailAppend = !flush; log.FailFlush = flush;
        using var tx = engine.Begin(); tx.SetName(1, "uncertain"); Assert.Throws<IOException>(() => tx.Commit());
        Assert.Null(before.Rows[1].Name); Assert.Throws<InvalidOperationException>(() => engine.Snapshot());
        log.FailAppend = log.FailFlush = false; var reopened = new LinksEngine<ulong>(log, options);
        Assert.Equal(flush ? "uncertain" : null, reopened.Snapshot().Rows[1].Name);
        using var following = reopened.Begin(); following.SetName(1, "following"); following.Commit(); Assert.Null(before.Rows[1].Name);
    }

    [Theory]
    [InlineData(StateStructure.Flat)]
    [InlineData(StateStructure.Shared)]
    public void ExistingPortableRecordsReopenWithoutChangingTheirWire(StateStructure structure)
    {
        using var vectors = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol", "engine-revision-vectors.json")));
        var log = new MemoryCommitLog(); foreach (var row in vectors.RootElement.GetProperty("valid").EnumerateArray()) log.Append(row.GetString()!);
        var engine = new LinksEngine<uint>(log, new() { StateStructure = structure, HistoryLimit = null });
        Assert.Equal("雪;|😀", engine.SnapshotAt(1).Rows[1].Name); Assert.True(engine.Snapshot().Rows.ContainsKey(uint.MaxValue));
        Assert.Equal(vectors.RootElement.GetProperty("valid").EnumerateArray().Select(row => row.GetString()), log.Read());
    }
}
