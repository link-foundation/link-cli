using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EnginePagedCompositionTests
{
    [Fact]
    public void EveryIndependentPolicyReopensGroupsReplicasAndFollowingWrites()
    {
        foreach (var structure in Enum.GetValues<StateStructure>())
            foreach (var encoding in Enum.GetValues<RevisionEncoding>())
                foreach (var validation in Enum.GetValues<ConstraintValidation>())
                    foreach (bool delta in new[] { false, true })
                    {
                        var primary = new PagedTestDevice(); var replica = new PagedTestDevice();
                        for (int round = 0; round < 2; round++)
                        {
                            ICommitLog log = new ReplicatedCommitLog(new PagedCommitLog(primary), new[] { new PagedCommitLog(replica) }, 2);
                            if (delta) log = new DeltaCommitLog(log);
                            var engine = new LinksEngine<ulong>(log, new()
                            {
                                StateStructure = structure,
                                RevisionEncoding = encoding,
                                ConstraintValidation = validation,
                                Isolation = IsolationLevel.Snapshot,
                                HistoryLimit = null,
                                RequireDurableFlush = true,
                                Constraints = new[] { EngineConstraint.References }
                            });
                            var before = engine.Snapshot(); Assert.Equal((ulong)round * 3, before.Revision); Assert.Equal(round * 3, before.Rows.Count);
                            Assert.All(before.Rows.Values, row => Assert.Contains(row.Name, new[] { "\ufeff雪\0😀", "second", "following" }));
                            primary.Calls.Clear(); replica.Calls.Clear();
                            using var a = engine.Begin(); ulong first = a.CreateLink(0, 0); a.SetName(first, "\ufeff雪\0😀");
                            using var b = engine.Begin(); ulong second = b.CreateLink(first, 0); b.SetName(second, "second");
                            Assert.Equal(new[] { before.Revision + 1, before.Revision + 2 }, engine.CommitGroup(new[] { a, b }).Select(result => result.Revision!.Value));
                            Assert.Equal(1, primary.Calls.Count(call => call == "syncData")); Assert.Equal(1, replica.Calls.Count(call => call == "syncData"));
                            Assert.Equal(before.Rows, engine.SnapshotAt(before.Revision).Rows);
                            using var invalid = engine.Begin(); invalid.CreateLink(999999, 0); Assert.Throws<InvalidOperationException>(() => invalid.Commit());
                            Assert.Equal(1, primary.Calls.Count(call => call == "syncData"));
                            using var discarded = engine.Begin(); discarded.CreateLink(0, 0); discarded.Rollback();
                            using var following = engine.Begin(); ulong third = following.CreateLink(second, 0); following.SetName(third, "following");
                            Assert.Equal(before.Revision + 3, following.Commit());
                            Assert.Equal(2, primary.Calls.Count(call => call == "syncData")); Assert.Equal(2, replica.Calls.Count(call => call == "syncData"));
                            Assert.Equal(new PagedCommitLog(primary.Crash()).Read(), new PagedCommitLog(replica.Crash()).Read());
                            Assert.Equal(before.Rows.Count + 3, engine.Snapshot().Rows.Count);
                        }
                    }
    }
}
