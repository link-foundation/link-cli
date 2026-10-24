using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineClusterTests
{
    private sealed class Store : IAtomicJournalStore
    {
        private readonly object _gate = new();
        private readonly Dictionary<string, JournalVersion> _values = new();
        private ulong _generation;
        public bool LoseReply;
        public bool SupportsDurableFlush => false;
        public Func<IReadOnlyList<string>, IReadOnlyList<JournalVersion>>? ReadOverride;
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys)
        { lock (_gate) return ReadOverride?.Invoke(keys) ?? keys.Select(key => _values.GetValueOrDefault(key) ?? new(0, Array.Empty<string>())).ToArray(); }
        public bool CompareExchange(IReadOnlyList<string> keys, IReadOnlyList<JournalVersion> expected, IReadOnlyList<IReadOnlyList<string>?> updates)
        {
            lock (_gate)
            {
                if (keys.Where((key, i) => (_values.GetValueOrDefault(key)?.Generation ?? 0) != expected[i].Generation).Any()) return false;
                var generation = ++_generation;
                for (var i = 0; i < keys.Count; i++) if (updates[i] is { } entries) _values[keys[i]] = new(generation, entries.ToArray());
                if (LoseReply) throw new IOException("response lost");
                return true;
            }
        }
    }

    [Fact]
    public void CrossShardCommitChecksReadOnlyDependenciesAndRejectsAllStaleWrites()
    {
        var store = new Store(); var first = new ClusterEngine<uint>(store); var second = new ClusterEngine<uint>(store);
        using (var seed = first.Begin("a", "b")) { seed.Shard("a").CreateLink(0, 0); seed.Shard("b").CreateLink(0, 0); seed.Commit(); }
        using var stale = first.Begin("a", "b");
        using (var fresh = second.Begin("b")) { fresh.Shard("b").SetName(1, "fresh"); fresh.Commit(); }
        stale.Shard("a").DeleteLink(1);
        Assert.Throws<InvalidOperationException>(() => stale.Commit());
        Assert.Throws<InvalidOperationException>(() => stale.Shard("a"));
        using var check = second.Begin("a", "b");
        Assert.NotNull(check.Shard("a").GetLink(1)); Assert.Equal("fresh", check.Shard("b").GetName(1));
    }

    [Fact]
    public void IndependentShardsCanCommitAndRollbackPublishesNothing()
    {
        var cluster = new ClusterEngine<uint>(new Store());
        using var a = cluster.Begin("a"); using var b = cluster.Begin("b");
        a.Shard("a").CreateLink(0, 0); b.Shard("b").CreateLink(0, 0); a.Commit(); b.Commit();
        using (var abandoned = cluster.Begin("a", "b")) { abandoned.Shard("a").DeleteLink(1); abandoned.Rollback(); }
        using var reader = cluster.Begin("a"); Assert.NotNull(reader.Shard("a").GetLink(1));
    }

    [Fact]
    public void UnknownCommitIsNotReplayedAndFreshSnapshotDiscoversAllShards()
    {
        var store = new Store(); var cluster = new ClusterEngine<uint>(store);
        using var tx = cluster.Begin("a", "b"); tx.Shard("a").CreateLink(0, 0); tx.Shard("b").CreateLink(0, 0);
        store.LoseReply = true;
        Assert.Contains("Indeterminate", Assert.Throws<IOException>(() => tx.Commit()).Message);
        Assert.Throws<InvalidOperationException>(() => tx.Commit());
        using var recovered = cluster.Begin("a", "b");
        Assert.NotNull(recovered.Shard("a").GetLink(1)); Assert.NotNull(recovered.Shard("b").GetLink(1));
    }

    [Fact]
    public void InvalidPoliciesKeysAndConstraintsNeverPublishPartialGroups()
    {
        var store = new Store();
        foreach (var options in new EngineOptions[] { new() { Concurrency = ConcurrencyControl.Exclusive }, new() { Isolation = IsolationLevel.Snapshot }, new() { RequireDurableFlush = true } })
            Assert.Throws<ArgumentException>(() => new ClusterEngine<uint>(store, options));
        var cluster = new ClusterEngine<uint>(store, new() { Constraints = new[] { EngineConstraint.References } });
        foreach (var keys in new[] { Array.Empty<string>(), new[] { "a", "a" }, new[] { "../a" }, Enumerable.Repeat("a", 33).ToArray() })
            Assert.Throws<ArgumentException>(() => cluster.Begin(keys));
        using var tx = cluster.Begin("a", "b"); Assert.Throws<ArgumentException>(() => tx.Shard("absent"));
        tx.Shard("a").CreateLink(0, 0); tx.Shard("b").CreateLink(999, 0);
        Assert.Throws<InvalidOperationException>(() => tx.Commit());
        using var check = cluster.Begin("a"); Assert.Null(check.Shard("a").GetLink(1));
    }

    [Fact]
    public void StandardLinksAndNamesApiRemainsPrivateAndClosesWithItsGroup()
    {
        var cluster = new ClusterEngine<uint>(new Store());
        using var group = cluster.Begin("a"); var shard = group.Shard("a");
        Assert.Equal(10u, shard.EnsureCreated(10));
        Assert.Equal(11u, shard.CreateLink(10, 0));
        Assert.Equal(0u, shard.UpdateLink(10, 11, 0).Source);
        Assert.Equal(10u, shard.SetName(10, "雪")); Assert.Equal(10u, shard.GetByName("雪"));
        shard.RemoveName(10); Assert.Null(shard.GetName(10)); Assert.Equal(0u, shard.GetByName("雪"));
        Assert.Equal(2u, shard.Count(null));
        var rows = new List<uint>();
        Assert.Equal(shard.Constants.Continue, shard.Each(null, link => { rows.Add(link![0]); return shard.Constants.Continue; }));
        Assert.Equal(new uint[] { 10, 11 }, rows);
        Assert.Equal(shard.Constants.Continue, shard.Create(null, null));
        Assert.Equal(shard.Constants.Break, shard.Update(new uint[] { 12 }, new uint[] { 12, 10, 11 }, (before, after) =>
        { Assert.Equal(0u, before![1]); Assert.Equal(10u, after![1]); return shard.Constants.Break; }));
        Assert.Equal(shard.Constants.Continue, shard.Delete(new uint[] { 12 }, null));
        Assert.Equal(11u, shard.DeleteLink(11).Index);
        using (var hidden = cluster.Begin("a")) Assert.Equal(0u, hidden.Shard("a").Count(null));
        group.Commit();
        Assert.Throws<InvalidOperationException>(() => shard.Count(null));
        Assert.Throws<InvalidOperationException>(() => shard.RemoveName(10));
        using var reader = cluster.Begin("a"); Assert.Equal(11u, reader.Shard("a").GetLink(10)!.Value.Source);
        Assert.Equal(1u, reader.Shard("a").Count(null));
    }

    [Fact]
    public void IncompleteAndCorruptCoordinatorSnapshotsDoNotPreventLaterValidGroups()
    {
        var store = new Store(); var cluster = new ClusterEngine<uint>(store);
        Assert.Throws<ArgumentException>(() => cluster.Begin(new string[] { null! }));
        Assert.Throws<ArgumentException>(() => cluster.Begin(new string('a', 129)));
        using (var boundary = cluster.Begin(new string('a', 128), "b.c_d-e")) boundary.Commit();
        store.ReadOverride = _ => Array.Empty<JournalVersion>();
        Assert.Throws<InvalidDataException>(() => cluster.Begin("a"));
        store.ReadOverride = keys => keys.Select(key => key == "bad" ? new JournalVersion(1, new[] { "corrupt" }) : new(0, Array.Empty<string>())).ToArray();
        Assert.Throws<InvalidDataException>(() => cluster.Begin("a", "bad"));
        store.ReadOverride = null;
        using var valid = cluster.Begin("a"); valid.Shard("a").CreateLink(0, 0); valid.Commit();
        using var reader = cluster.Begin("a"); Assert.Equal(1u, reader.Shard("a").Count(null));
    }
}
