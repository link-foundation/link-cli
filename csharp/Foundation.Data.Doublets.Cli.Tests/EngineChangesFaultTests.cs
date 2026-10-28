using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineChangesFaultTests
{
    private sealed class Image
    {
        internal List<string> Durable = new(), Staged = new();
        internal string Mode = "";
        internal int Prefix, Appends, Flushes;
    }
    private sealed class Log(Image image) : ICommitLog
    {
        public bool SupportsDurableFlush => false;
        public IReadOnlyList<string> Read() => image.Durable.ToArray();
        public void Append(string record)
        {
            image.Appends++; bool failing = image.Appends == 2 && image.Mode.StartsWith("append", StringComparison.Ordinal);
            if (!failing || image.Mode == "append-after") image.Staged.Add(record);
            if (failing) { image.Durable = image.Staged.Take(1 + image.Prefix).ToList(); throw new IOException("Independent append image."); }
        }
        public void Flush()
        {
            image.Flushes++;
            if (image.Mode == "flush") { image.Durable = image.Staged.Take(1 + image.Prefix).ToList(); throw new IOException("Independent flush image."); }
            image.Durable = image.Staged.ToList();
        }
    }
    [Theory]
    [InlineData(StateStructure.Flat)]
    [InlineData(StateStructure.Shared)]
    [InlineData(StateStructure.SortedRuns)]
    public void UncertainChangeGroupsRecoverEveryDurablePrefixAndPreserveTheOldVisibleState(StateStructure structure)
    {
        foreach (bool delta in new[] { false, true })
            foreach (var (mode, maxPrefix) in new[] { ("append-before", 1), ("append-after", 2), ("flush", 2) })
                for (int prefix = 0; prefix <= maxPrefix; prefix++)
                    foreach (var validation in Enum.GetValues<ConstraintValidation>())
                    {
                        var image = new Image(); ICommitLog log = new Log(image); if (delta) log = new DeltaCommitLog(log);
                        var options = new EngineOptions { StateStructure = structure, RevisionEncoding = RevisionEncoding.Changes, Isolation = IsolationLevel.ReadCommitted, HistoryLimit = null, ConstraintValidation = validation, Constraints = new[] { EngineConstraint.UniqueNames } };
                        var engine = new LinksEngine<uint>(log, options);
                        using (var seed = engine.Begin()) { seed.EnsureCreated(1); seed.EnsureCreated(2); seed.Commit(); }
                        var pinned = engine.Snapshot(); using var observer = engine.Begin();
                        image.Mode = mode; image.Prefix = prefix; image.Appends = 0; image.Flushes = 0;
                        using var first = engine.Begin(); using var second = engine.Begin(); first.SetName(1, "one 雪\0"); second.SetName(2, "");
                        Assert.All(engine.CommitGroup(new[] { first, second }), outcome => Assert.Equal(GroupCommitStatus.Indeterminate, outcome.Status));
                        Assert.Null(pinned.Rows[1].Name); Assert.Null(pinned.Rows[2].Name); Assert.Null(observer.GetName(1)); Assert.Null(observer.GetName(2));
                        Assert.Throws<InvalidOperationException>(() => engine.Snapshot()); Assert.Throws<InvalidOperationException>(() => engine.Begin());
                        Assert.Equal(2, image.Appends); Assert.Equal(mode == "flush" ? 1 : 0, image.Flushes); observer.Rollback();
                        image.Staged = image.Durable.ToList(); image.Mode = "";
                        // Change records pass through the physical delta adapter, so
                        // a plain reopen also excludes hidden full-image dependence.
                        var reopened = new LinksEngine<uint>(new Log(image), options); var actual = reopened.Snapshot();
                        Assert.Equal(1UL + (ulong)prefix, actual.Revision); Assert.Equal(prefix >= 1 ? "one 雪\0" : null, actual.Rows[1].Name); Assert.Equal(prefix >= 2 ? "" : null, actual.Rows[2].Name);
                        Assert.Null(reopened.SnapshotAt(1).Rows[1].Name);
                        using (var following = reopened.Begin()) { following.SetName(1, "following"); following.Commit(); }
                        Assert.Null(pinned.Rows[1].Name);
                        Assert.Equal("following", new LinksEngine<uint>(new Log(image)).Snapshot().Rows[1].Name);
                    }
    }
}
