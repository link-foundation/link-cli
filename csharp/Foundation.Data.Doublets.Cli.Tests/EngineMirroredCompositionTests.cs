using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineMirroredCompositionTests
{
    [Fact]
    public void EveryIndependentPolicyReopensGroupsReplicasAndFollowingWrites()
    {
        foreach (var structure in Enum.GetValues<StateStructure>())
            foreach (var encoding in Enum.GetValues<RevisionEncoding>())
                foreach (var validation in Enum.GetValues<ConstraintValidation>())
                    foreach (bool delta in new[] { false, true })
                    {
                        var primary = new MirroredTestDevice.Pair(new(), new()); var replica = new MirroredTestDevice.Pair(new(), new());
                        for (int round = 0; round < 2; round++)
                        {
                            ICommitLog log = new ReplicatedCommitLog(primary.Open(), new[] { replica.Open() }, 2);
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
                            primary.Mutations.Calls.Clear(); replica.Mutations.Calls.Clear();
                            using var a = engine.Begin(); ulong first = a.CreateLink(0, 0); a.SetName(first, "\ufeff雪\0😀");
                            using var b = engine.Begin(); ulong second = b.CreateLink(first, 0); b.SetName(second, "second");
                            Assert.Equal(new[] { before.Revision + 1, before.Revision + 2 }, engine.CommitGroup(new[] { a, b }).Select(result => result.Revision!.Value));
                            Assert.Equal(2, primary.Mutations.Calls.Count(call => call.EndsWith(":syncData", StringComparison.Ordinal))); Assert.Equal(2, replica.Mutations.Calls.Count(call => call.EndsWith(":syncData", StringComparison.Ordinal)));
                            Assert.Equal(before.Rows, engine.SnapshotAt(before.Revision).Rows);
                            using var invalid = engine.Begin(); invalid.CreateLink(999999, 0); Assert.Throws<InvalidOperationException>(() => invalid.Commit());
                            Assert.Equal(2, primary.Mutations.Calls.Count(call => call.EndsWith(":syncData", StringComparison.Ordinal)));
                            using var discarded = engine.Begin(); discarded.CreateLink(0, 0); discarded.Rollback();
                            using var following = engine.Begin(); ulong third = following.CreateLink(second, 0); following.SetName(third, "following");
                            Assert.Equal(before.Revision + 3, following.Commit());
                            Assert.Equal(4, primary.Mutations.Calls.Count(call => call.EndsWith(":syncData", StringComparison.Ordinal))); Assert.Equal(4, replica.Mutations.Calls.Count(call => call.EndsWith(":syncData", StringComparison.Ordinal)));
                            Assert.Equal(primary.Crash().Open().Read(), replica.Crash().Open().Read());
                            Assert.Equal(before.Rows.Count + 3, engine.Snapshot().Rows.Count);
                        }
                    }
    }
}
