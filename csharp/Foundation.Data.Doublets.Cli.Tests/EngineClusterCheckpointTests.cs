using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public partial class EngineClusterTests
{
    [Fact]
    public void SelectedCheckpointsPreserveWritesWhenJournalsShrinkOrKeepLength()
    {
        foreach (int? history in new int?[] { 0, 2, null })
        {
            var store = new Store(); var cluster = new ClusterEngine<uint>(store, new() { HistoryLimit = history }, journalCheckpointInterval: 2);
            Assert.Throws<ArgumentOutOfRangeException>(() => new ClusterEngine<uint>(store, journalCheckpointInterval: 0));
            for (int revision = 1; revision <= 6; revision++)
            {
                using var tx = cluster.Begin("a", "b");
                foreach (string key in new[] { "a", "b" })
                {
                    if (revision == 1) tx.Shard(key).CreateLink(0, 0);
                    tx.Shard(key).SetName(1, $"{key} {revision}");
                }
                tx.Commit(); var records = store.Read(new[] { "a" })[0].Entries;
                if (revision % 2 == 0)
                {
                    Assert.StartsWith("engine3|", records[0]);
                    Assert.Equal(history == 0 ? 1 : history ?? revision + 1, records.Count);
                }
                var log = new MemoryCommitLog(); foreach (var entry in records) log.Append(entry);
                var reopened = new LinksEngine<uint>(log, new() { HistoryLimit = null });
                Assert.Equal((ulong)revision, reopened.Snapshot().Revision); Assert.Equal($"a {revision}", reopened.Snapshot().Rows[1].Name);
                if (history is not null && revision >= 2) Assert.Throws<KeyNotFoundException>(() => reopened.SnapshotAt(0));
            }
            var before = store.Read(new[] { "a", "b" });
            using (var readOnly = cluster.Begin("a", "b")) readOnly.Commit();
            Assert.Equal(before, store.Read(new[] { "a", "b" }));
        }
        var plainStore = new Store(); var plain = new ClusterEngine<uint>(plainStore);
        for (int id = 1; id <= 3; id++) { using var tx = plain.Begin("a"); Assert.Equal((uint)id, tx.Shard("a").CreateLink(0, 0)); tx.Commit(); }
        var journal = plainStore.Read(new[] { "a" })[0].Entries;
        Assert.Equal(3, journal.Count); Assert.StartsWith("engine1|", journal[0]);
    }

    [Fact]
    public void PartitionedCheckpointsKeepOldObjectsImmutable()
    {
        var catalog = new Store(); var data = new Store();
        var objects = new PartitionedJournalStore(catalog, new Dictionary<string, IAtomicJournalStore> { ["data"] = data }, _ => "data", 4096);
        var cluster = new ClusterEngine<uint>(objects, journalCheckpointInterval: 1);
        using (var seed = cluster.Begin("a")) { seed.Shard("a").CreateLink(0, 0); seed.Commit(); }
        var originalPointer = catalog.Read(new[] { "a" })[0].Entries;
        var original = data.Read(new[] { originalPointer[2] })[0];
        using (var next = cluster.Begin("a")) { next.Shard("a").SetName(1, "next"); next.Commit(); }
        Assert.Equal(original, data.Read(new[] { originalPointer[2] })[0]);
        Assert.NotEqual(originalPointer[2], catalog.Read(new[] { "a" })[0].Entries[2]);
        using var check = cluster.Begin("a"); Assert.Equal("next", check.Shard("a").GetName(1));
    }

    [Fact]
    public void CompactedGroupsKeepConflictsAndBeforeAfterUnknownOutcomes()
    {
        foreach (bool after in new[] { false, true })
        {
            var store = new Store(); var cluster = new ClusterEngine<uint>(store, journalCheckpointInterval: 1);
            using (var seed = cluster.Begin("a", "b")) { seed.Shard("a").CreateLink(0, 0); seed.Shard("b").CreateLink(0, 0); seed.Commit(); }
            using var stale = cluster.Begin("a", "b");
            using (var fresh = cluster.Begin("b")) { fresh.Shard("b").SetName(1, "fresh"); fresh.Commit(); }
            stale.Shard("a").DeleteLink(1); Assert.Throws<InvalidOperationException>(() => stale.Commit());
            using var unknown = cluster.Begin("a", "b"); foreach (string key in new[] { "a", "b" }) unknown.Shard(key).SetName(1, "unknown");
            store.LoseReply = after; store.FailBefore = !after;
            Assert.Contains("Indeterminate", Assert.Throws<IOException>(() => unknown.Commit()).Message);
            Assert.Throws<InvalidOperationException>(() => unknown.Commit()); Assert.Throws<InvalidOperationException>(() => unknown.Shard("a"));
            store.LoseReply = false; store.FailBefore = false;
            using var check = cluster.Begin("a", "b"); Assert.NotNull(check.Shard("a").GetLink(1));
            Assert.Equal(after ? "unknown" : null, check.Shard("a").GetName(1));
            Assert.Equal(after ? "unknown" : "fresh", check.Shard("b").GetName(1));
            check.Shard("a").SetName(1, "following"); check.Commit();
            using var following = cluster.Begin("a"); Assert.Equal("following", following.Shard("a").GetName(1));
        }
    }
}
