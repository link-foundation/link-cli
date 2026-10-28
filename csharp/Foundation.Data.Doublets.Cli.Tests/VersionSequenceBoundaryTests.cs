using System.Globalization;
using Platform.Data;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class VersionSequenceBoundaryTests
{
    private sealed class BoundedMarkers(string path) : NamedTypesDecorator<uint>(path)
    {
        public int TransitionWrites;
        public override uint SetName(uint link, string name)
        {
            const string prefix = "__vc:trans:";
            if (name.StartsWith(prefix, StringComparison.Ordinal))
            {
                // Bound the pre-fix wraparound/sparse-range probes before they
                // can allocate an unbounded number of mapped metadata rows.
                var end = name.IndexOf(":branch=", StringComparison.Ordinal);
                var sequence = long.Parse(name.AsSpan(prefix.Length, end - prefix.Length), CultureInfo.InvariantCulture);
                if (++TransitionWrites > 8 || sequence < 1)
                    throw new IOException("version attribution exceeded the finite marker budget");
            }
            return base.SetName(link, name);
        }
    }

    private static void SeedHighSequence(INamedTypesLinks<uint> log, uint baseline, long sequence)
    {
        var id = Guid.NewGuid();
        var state = new Link<uint>(baseline, 11u, 12u);
        var record = new Transition<uint>(id, sequence, DateTimeOffset.UnixEpoch,
            TransitionKind.Update, state, state);
        foreach (var marker in new[] { "__transactions:transition:" + record.Serialize(),
            "__transactions:commit:" + id.ToString("N"), "__transactions:applied:" + sequence })
            log.SetName(log.CreateAndUpdate(0u, 0u), marker);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void FinalAvailableSequencePublishesWithoutWrapping(bool explicitTransaction)
    {
        var directory = Directory.CreateTempSubdirectory("version-sequence-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        var branchPath = Path.Combine(directory, "branches.links");
        try
        {
            uint baseline, created;
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var branches = new BoundedMarkers(branchPath))
            using (var tx = new TransactionsDecorator(data, log))
            using (var vc = new VersionControlDecorator(tx, branches))
            {
                baseline = vc.CreateAndUpdate(11u, 12u);
                SeedHighSequence(log, baseline, long.MaxValue - 1);
                data.Flush(); log.Flush();
            }
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var branches = new BoundedMarkers(branchPath))
            using (var tx = new TransactionsDecorator(data, log))
            using (var vc = new VersionControlDecorator(tx, branches))
            {
                Assert.Equal(long.MaxValue - 1, tx.LastLoggedSequence);
                using var pending = explicitTransaction ? vc.BeginTransaction() : null;
                created = 0;
                vc.Create(null, (_, after) =>
                {
                    created = Assert.IsAssignableFrom<IList<uint>>(after)[0];
                    return vc.Constants.Continue;
                });
                pending?.Commit();
                Assert.True(vc.Exists(created), $"Created address {created} must be visible.");
                Assert.Equal(long.MaxValue, vc.CurrentSequence);
                Assert.Equal(long.MaxValue, vc.ListBranches().Single().Head);
                Assert.Equal(1, branches.TransitionWrites);
                vc.Tag("last");
                Assert.Throws<InvalidOperationException>(() => vc.Create(null, null));
                Assert.Equal(2u, vc.Count());
            }
            using var recoveredData = new NamedTypesDecorator<uint>(dataPath);
            using var recoveredLog = new NamedTypesDecorator<uint>(logPath);
            using var recoveredBranches = new BoundedMarkers(branchPath);
            using var recoveredTx = new TransactionsDecorator(recoveredData, recoveredLog);
            using var recovered = new VersionControlDecorator(recoveredTx, recoveredBranches);
            Assert.Equal(long.MaxValue, recovered.CurrentSequence);
            Assert.Equal(long.MaxValue, recovered.ListBranches().Single().Head);
            Assert.True(recovered.TryGetTag("last", out var last)); Assert.Equal(long.MaxValue, last);
            Assert.True(recovered.Exists(baseline), "Baseline must survive reopening.");
            Assert.True(recovered.Exists(created), "Final creation must survive reopening.");
            recovered.Checkout(2);
            Assert.Equal(1u, recovered.Count()); Assert.False(recovered.Exists(created));
            recovered.Checkout(long.MaxValue);
            Assert.Equal(2u, recovered.Count()); Assert.True(recovered.Exists(created));
            Assert.Throws<InvalidOperationException>(() => recovered.Create(null, null));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    [Fact]
    public void InitialVersionHistoryVisitsRecordedSequencesWithoutFillingGaps()
    {
        var directory = Directory.CreateTempSubdirectory("version-sparse-").FullName;
        var dataPath = Path.Combine(directory, "data.links");
        var logPath = Path.Combine(directory, "log.links");
        var branchPath = Path.Combine(directory, "branches.links");
        try
        {
            uint baseline;
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var tx = new TransactionsDecorator(data, log))
            {
                baseline = tx.CreateAndUpdate(11u, 12u);
                SeedHighSequence(log, baseline, long.MaxValue);
                data.Flush(); log.Flush();
            }
            using (var data = new NamedTypesDecorator<uint>(dataPath))
            using (var log = new NamedTypesDecorator<uint>(logPath))
            using (var branches = new BoundedMarkers(branchPath))
            using (var tx = new TransactionsDecorator(data, log))
            using (var vc = new VersionControlDecorator(tx, branches))
            {
                Assert.Equal(long.MaxValue, vc.CurrentSequence);
                Assert.Equal(3, branches.TransitionWrites);
                Assert.Equal(1u, vc.Count());
                vc.Checkout(2); Assert.Equal(1u, vc.Count());
                Assert.Equal(11u, vc.GetSource(baseline)); Assert.Equal(12u, vc.GetTarget(baseline));
                vc.Checkout(long.MaxValue); Assert.Equal(1u, vc.Count());
                Assert.Throws<InvalidOperationException>(() => vc.Checkout(3));
                vc.Tag("sparse-last");
            }
            using var recoveredData = new NamedTypesDecorator<uint>(dataPath);
            using var recoveredLog = new NamedTypesDecorator<uint>(logPath);
            using var recoveredBranches = new BoundedMarkers(branchPath);
            using var recoveredTx = new TransactionsDecorator(recoveredData, recoveredLog);
            using var recovered = new VersionControlDecorator(recoveredTx, recoveredBranches);
            Assert.Equal(long.MaxValue, recovered.CurrentSequence);
            Assert.True(recovered.TryGetTag("sparse-last", out var last)); Assert.Equal(long.MaxValue, last);
            Assert.Equal(0, recoveredBranches.TransitionWrites);
            Assert.Equal(1u, recovered.Count());
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
