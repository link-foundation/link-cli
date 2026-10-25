using System.Collections.Immutable;
using System.Reflection;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineSharedStateTests
{
    private static EngineOptions Options(StateStructure structure) => new() { StateStructure = structure, HistoryLimit = null };

    [Theory]
    [InlineData(StateStructure.Flat)]
    [InlineData(StateStructure.Shared)]
    public void SelectedStructureSharesImmutableNodesAndPreservesPinnedSnapshots(StateStructure structure)
    {
        var log = new MemoryCommitLog(); var engine = new LinksEngine<ulong>(log, Options(structure));
        using (var seed = engine.Begin())
        {
            for (ulong id = 1; id <= 256; id++) { seed.EnsureCreated(id); seed.SetName(id, $"row {id} 雪"); }
            seed.Commit();
        }
        var first = engine.Snapshot(); var previous = first;
        for (ulong revision = 1; revision <= 16; revision++)
        {
            using var tx = engine.Begin(); tx.UpdateLink(1, revision, 0); tx.Commit();
            var current = engine.Snapshot();
            for (ulong id = 2; id <= 256; id++) Assert.Equal(previous.Rows[id], current.Rows[id]);
            Assert.Equal(0UL, first.Rows[1].Link.Source);
            if (structure == StateStructure.Shared)
            {
                var oldNodes = Nodes(Assert.IsType<ImmutableSortedDictionary<ulong, EngineRow<ulong>>>(previous.Rows));
                var newNodes = Nodes(Assert.IsType<ImmutableSortedDictionary<ulong, EngineRow<ulong>>>(current.Rows));
                Assert.Equal(256, oldNodes.Count); Assert.Equal(256, newNodes.Count);
                Assert.InRange(oldNodes.Intersect(newNodes, ReferenceEqualityComparer.Instance).Count(), 224, 255);
            }
            else Assert.IsNotType<ImmutableSortedDictionary<ulong, EngineRow<ulong>>>(current.Rows);
            previous = current;
        }
        Assert.Same(first, engine.SnapshotAt(1));
        if (structure == StateStructure.Shared)
        {
            var reopened = new LinksEngine<ulong>(log, Options(structure));
            var oldNodes = Nodes(Assert.IsType<ImmutableSortedDictionary<ulong, EngineRow<ulong>>>(reopened.SnapshotAt(1).Rows));
            var nextNodes = Nodes(Assert.IsType<ImmutableSortedDictionary<ulong, EngineRow<ulong>>>(reopened.SnapshotAt(2).Rows));
            Assert.InRange(oldNodes.Intersect(nextNodes, ReferenceEqualityComparer.Instance).Count(), 224, 255);
            using var reader = engine.Begin();
            var view = (EngineRows<ulong>)typeof(EngineSession<ulong>).GetMethod("View", BindingFlags.NonPublic | BindingFlags.Instance)!.Invoke(reader, null)!;
            Assert.Same(engine.Snapshot().Rows, view.Freeze());
            Assert.Throws<InvalidOperationException>(() => view.Set(1, first.Rows[1]));
            Assert.Throws<InvalidOperationException>(() => view.Remove(1));
        }
        // The public immutable dictionary cannot mutate its frozen snapshots.
        if (first.Rows is ImmutableSortedDictionary<ulong, EngineRow<ulong>> map)
        {
            Assert.Equal(255, map.Remove(1).Count); Assert.Equal(256, first.Rows.Count);
        }
        else Assert.Throws<NotSupportedException>(() => ((IDictionary<ulong, EngineRow<ulong>>)first.Rows).Clear());
    }

    // Bounded component-level allocation identity, independent of wall clocks.
    // .NET 10's immutable AVL nodes use these private links. A runtime change
    // fails this cost control explicitly instead of silently weakening it.
    private static HashSet<object> Nodes(ImmutableSortedDictionary<ulong, EngineRow<ulong>> map)
    {
        const BindingFlags flags = BindingFlags.NonPublic | BindingFlags.Instance;
        var root = map.GetType().GetField("_root", flags)!.GetValue(map)!;
        var left = root.GetType().GetField("_left", flags)!; var right = root.GetType().GetField("_right", flags)!;
        var result = new HashSet<object>(ReferenceEqualityComparer.Instance); var pending = new Stack<object>(); pending.Push(root);
        while (pending.TryPop(out var node))
        {
            var l = left.GetValue(node); if (l is null) continue; // empty sentinel
            Assert.True(result.Add(node)); Assert.InRange(result.Count, 1, 256);
            pending.Push(l); pending.Push(right.GetValue(node)!);
        }
        return result;
    }

    [Theory]
    [InlineData(StateStructure.Flat)]
    [InlineData(StateStructure.Shared)]
    public void BothStructuresMatchAFiniteIndependentRowsAndNamesOracle(StateStructure structure)
    {
        var log = new MemoryCommitLog(); var engine = new LinksEngine<ulong>(log, Options(structure));
        var expected = new SortedDictionary<ulong, (ulong Source, ulong Target, string? Name)>();
        var saved = new List<(EngineSnapshot<ulong> Snapshot, SortedDictionary<ulong, (ulong Source, ulong Target, string? Name)> Expected)>();
        for (ulong step = 0; step < 96; step++)
        {
            var id = 1 + step * 7 % 13; using var tx = engine.Begin(); var candidate = new SortedDictionary<ulong, (ulong Source, ulong Target, string? Name)>(expected);
            switch (step % 4)
            {
                case 0:
                case 1:
                    tx.EnsureCreated(id); tx.UpdateLink(id, step, id);
                    string? name = step % 3 != 0 ? $"{step};|雪\0" : null;
                    if (name is null) tx.RemoveName(id); else tx.SetName(id, name);
                    candidate[id] = (step, id, name); break;
                case 2: if (candidate.Remove(id)) tx.DeleteLink(id); break;
                default: Assert.Equal(expected.ContainsKey(id), tx.GetLink(id).HasValue); break;
            }
            if (step % 7 == 0) tx.Rollback(); else { tx.Commit(); expected = candidate; }
            var current = engine.Snapshot();
            Assert.Equal(expected, current.Rows.ToDictionary(pair => pair.Key, pair => (pair.Value.Link.Source, pair.Value.Link.Target, pair.Value.Name)));
            saved.Add((current, new(expected)));
        }
        foreach (var (snapshot, oracle) in saved)
        {
            Assert.Equal(oracle.Count, snapshot.Rows.Count);
            foreach (var (id, value) in oracle) Assert.Equal(value, (snapshot.Rows[id].Link.Source, snapshot.Rows[id].Link.Target, snapshot.Rows[id].Name));
        }
        var records = log.Read().ToArray();
        foreach (var reopen in Enum.GetValues<StateStructure>())
        {
            var journal = new MemoryCommitLog(); foreach (var record in records) journal.Append(record);
            var reopened = new LinksEngine<ulong>(journal, Options(reopen));
            Assert.Equal(engine.Snapshot().Rows, reopened.Snapshot().Rows);
            using var following = reopened.Begin(); following.EnsureCreated(ulong.MaxValue); following.Commit();
            Assert.True(reopened.Snapshot().Rows.ContainsKey(ulong.MaxValue));
        }
        Assert.Throws<ArgumentException>(() => new LinksEngine<ulong>(new MemoryCommitLog(), new() { StateStructure = (StateStructure)99 }));
    }
}
