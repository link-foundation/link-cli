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
        public IReadOnlyList<JournalVersion> Read(IReadOnlyList<string> keys)
        { lock (_gate) return keys.Select(key => _values.GetValueOrDefault(key) ?? new(0, Array.Empty<string>())).ToArray(); }
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
}
