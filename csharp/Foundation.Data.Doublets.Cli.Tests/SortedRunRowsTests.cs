using System.Collections;
using System.Reflection;
using Foundation.Data.Doublets.Cli.Engine;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class SortedRunRowsTests
{
    private static EngineRow<ulong> Row(ulong id, ulong value) => new(new Link<ulong>(id, value, 0), $"row {value} 雪\0");

    [Fact]
    public void FiniteEditsMatchAnIndependentMapAndPreserveEveryPinnedView()
    {
        var runs = new SortedRunRows<ulong>(); var expected = new SortedDictionary<ulong, EngineRow<ulong>>();
        var saved = new List<(SortedRunRows<ulong>, SortedDictionary<ulong, EngineRow<ulong>>)>();
        for (ulong step = 0; step < 1024; step++)
        {
            var id = step * 17 % 31; EngineRow<ulong>? value = step % 5 == 0 ? null : Row(id, step);
            runs = runs.With(id, value);
            if (value is { } row) expected[id] = row; else expected.Remove(id);
            for (ulong key = 0; key <= 32; key++)
            {
                Assert.Equal(expected.TryGetValue(key, out var want), runs.TryGetValue(key, out var actual));
                Assert.Equal(want, actual); Assert.Equal(expected.ContainsKey(key), runs.ContainsKey(key));
            }
            Assert.Equal(expected.Count, runs.Count); Assert.Equal(expected, runs);
            Assert.Equal(expected.Keys, runs.Keys); Assert.Equal(expected.Values, runs.Values);
            saved.Add((runs, new(expected)));
        }
        foreach (var (snapshot, oracle) in saved) Assert.Equal(oracle, snapshot);
    }

    [Fact]
    public void TombstonesHideOlderRowsAndFullCompactionCannotResurrectThem()
    {
        var runs = new SortedRunRows<ulong>().With(1, Row(1, 11)).With(2, Row(2, 22)); var pinned = runs;
        runs = runs.With(1, null); Assert.False(runs.ContainsKey(1)); Assert.Throws<KeyNotFoundException>(() => runs[1]);
        Assert.Equal(new[] { 2UL }, runs.Keys); Assert.Single(runs);
        runs = runs.With(2, null); Assert.Empty(runs);
        Assert.Empty(Levels(runs)); Assert.Equal(Row(1, 11), pinned[1]); Assert.Equal(Row(2, 22), pinned[2]);
        runs = runs.With(1, Row(1, 33)); Assert.Equal(Row(1, 33), runs[1]); Assert.Single(runs);
        var enumerator = ((IEnumerable)runs).GetEnumerator(); Assert.True(enumerator.MoveNext()); Assert.False(enumerator.MoveNext());
    }

    [Fact]
    public void NoOpsPreserveTheExactRootAndPointReadsDoNotMaterializeScans()
    {
        var runs = new SortedRunRows<ulong>();
        for (ulong id = 1; id <= 1024; id++) runs = runs.With(id, Row(id, id));
        Assert.False(Cache(runs).IsValueCreated);
        Assert.Equal(Row(100, 100), runs[100]); Assert.Equal(1024, runs.Count);
        Assert.False(Cache(runs).IsValueCreated);
        Assert.Same(runs, runs.With(100, Row(100, 100))); Assert.Same(runs, runs.With(0, null));
        var next = runs.With(ulong.MaxValue, Row(ulong.MaxValue, 0));
        Assert.Same(Levels(runs).GetValue(10), Levels(next).GetValue(10));
        Assert.False(Cache(next).IsValueCreated); Assert.False(runs.ContainsKey(ulong.MaxValue));
        Assert.True(next.ContainsKey(ulong.MaxValue)); Assert.False(typeof(IDictionary).IsAssignableFrom(next.GetType()));
        Assert.Equal(1024, runs.ToArray().Length); Assert.True(Cache(runs).IsValueCreated);
        Assert.Same(runs, runs.With(100, Row(100, 100))); Assert.False(Cache(next).IsValueCreated);
    }

    [Fact]
    public void CandidateConversionsAndFreezingPreserveAllThreeRepresentations()
    {
        foreach (var source in Enum.GetValues<StateStructure>())
            foreach (var target in Enum.GetValues<StateStructure>())
            {
                var seed = new EngineRows<ulong>(source); seed.Set(1, Row(1, 11)); seed.Set(2, Row(2, 22));
                var first = seed.Freeze(); var candidate = new EngineRows<ulong>(first, target);
                candidate.Remove(1); candidate.Set(2, Row(2, 33)); candidate.Set(ulong.MaxValue, Row(ulong.MaxValue, 44));
                var current = candidate.Freeze(); Assert.Equal(Row(1, 11), first[1]); Assert.Equal(Row(2, 22), first[2]);
                Assert.False(current.ContainsKey(1)); Assert.Equal(Row(2, 33), current[2]); Assert.Equal(2, current.Count);
                Assert.Throws<InvalidOperationException>(() => candidate.Set(3, Row(3, 3)));
                Assert.Throws<InvalidOperationException>(() => candidate.Remove(2));
                var replay = new EngineRows<ulong>(current, target, first).Freeze(); Assert.Equal(current, replay);
                var empty = new EngineRows<ulong>(new SortedDictionary<ulong, EngineRow<ulong>>(), target, current).Freeze();
                Assert.Empty(empty); Assert.Equal(2, current.Count);
            }
    }

    private static Array Levels(SortedRunRows<ulong> rows) =>
        (Array)typeof(SortedRunRows<ulong>).GetField("_levels", BindingFlags.NonPublic | BindingFlags.Instance)!.GetValue(rows)!;
    private static Lazy<SortedDictionary<ulong, EngineRow<ulong>>> Cache(SortedRunRows<ulong> rows) =>
        (Lazy<SortedDictionary<ulong, EngineRow<ulong>>>)typeof(SortedRunRows<ulong>).GetField("_ordered", BindingFlags.NonPublic | BindingFlags.Instance)!.GetValue(rows)!;
}
