using Foundation.Data.Doublets.Cli.Engine;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public sealed class QueryAllocationTests
{
    private static Link<uint>[] Rows(EngineSession<uint> tx)
    {
        var rows = new List<Link<uint>>();
        tx.Each(null, raw => { rows.Add(new(raw)); return tx.Constants.Continue; });
        return rows.ToArray();
    }

    [Fact]
    public void AnonymousForwardReferenceUsesThePredictedIdentityWithoutProbeRows()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var tx = engine.Begin();
        var observed = new List<Link<uint>>();
        AdvancedMixedQueryProcessor.ProcessQuery(tx, new()
        {
            Query = "(() ((1 1)))",
            ChangesHandler = (_, after) =>
            {
                observed.Add(new(after));
                return tx.Constants.Continue;
            },
        });
        Assert.Equal(new[] { new Link<uint>(1, 1, 1) }, Rows(tx));
        Assert.Equal(new[] { new Link<uint>(1, 0, 0), new Link<uint>(1, 1, 1) }, observed);
        Assert.Empty(engine.Snapshot().Rows);
        tx.Commit();
        Assert.Equal(new[] { new Link<uint>(1, 1, 1) }, engine.Snapshot().Rows.Values.Select(row => row.Link));
    }

    [Theory]
    [InlineData("() ((1 2) (2 2))", "1:1:2,2:2:2")]
    [InlineData("() ((1: 1 1) (2 2))", "1:1:1,2:2:2")]
    [InlineData("() ((foo: (1 1) (1 1)))", "1:1:1")]
    [InlineData("() ((foo: foo foo) (2 2))", "1:1:1,2:2:2")]
    [InlineData("() ((1 1) (2: 2 2) (3 3))", "1:1:1,2:2:2,3:3:3")]
    [InlineData("() ((100: 100 100) (101 101))", "100:100:100,101:101:101")]
    public void PredictionFollowsExplicitDefinitionsNamesAndReusedPairs(string query, string expected)
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var tx = engine.Begin();
        AdvancedMixedQueryProcessor.ProcessQuery(tx, query);
        Assert.Equal(expected, string.Join(",", Rows(tx).Select(row => $"{row.Index}:{row.Source}:{row.Target}")));
        Assert.Empty(engine.Snapshot().Rows);
        tx.Commit();
        Assert.Equal(expected, string.Join(",", engine.Snapshot().Rows.Values.Select(row => $"{row.Link.Index}:{row.Link.Source}:{row.Link.Target}")));
    }

    [Fact]
    public void FailedReferenceValidationKeepsReservedIdsSeparateFromOtherSessions()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog(), new() { Isolation = IsolationLevel.Snapshot });
        using var first = engine.Begin();
        Assert.Throws<InvalidOperationException>(() => AdvancedMixedQueryProcessor.ProcessQuery(first, "() ((9 9))"));
        Assert.Empty(Rows(first));
        using var second = engine.Begin();
        AdvancedMixedQueryProcessor.ProcessQuery(second, "() ((2 2))");
        second.Commit();
        AdvancedMixedQueryProcessor.ProcessQuery(first, "() ((1 1))");
        first.Commit();
        Assert.Equal(new[] { new Link<uint>(1, 1, 1), new Link<uint>(2, 2, 2) }, engine.Snapshot().Rows.Values.Select(row => row.Link));
    }

    [Fact]
    public void ExplicitEnsureRetiresOnlyItsReservationAndAutoCreationKeepsThePrefix()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using (var tx = engine.Begin())
        {
            Assert.Throws<InvalidOperationException>(() => AdvancedMixedQueryProcessor.ProcessQuery(tx, "() ((9 9))"));
            tx.EnsureCreated(1);
            tx.UpdateLink(1, 1, 1);
            Assert.Equal(2u, tx.CreateLink(0, 0));
            Assert.Equal(new Link<uint>(1, 1, 1), tx.GetLink(1));
        }
        Assert.Empty(engine.Snapshot().Rows);
        var fresh = new LinksEngine<uint>(new MemoryCommitLog());
        using var auto = fresh.Begin();
        AdvancedMixedQueryProcessor.ProcessQuery(auto, new()
        {
            Query = "() ((100 1))",
            AutoCreateMissingReferences = true,
        });
        Assert.Equal(new[] { new Link<uint>(1, 100, 1), new Link<uint>(100, 100, 100) }, Rows(auto));
        Assert.Empty(fresh.Snapshot().Rows);
    }

    [Fact]
    public void InvalidSyntaxAndExistingPairsDoNotConsumeExtraAddresses()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var tx = engine.Begin();
        Assert.ThrowsAny<Exception>(() => AdvancedMixedQueryProcessor.ProcessQuery(tx, "(() (("));
        Assert.Equal(1u, tx.CreateLink(1, 1));
        AdvancedMixedQueryProcessor.ProcessQuery(tx, "() ((1 1) (2 2))");
        Assert.Equal(3u, tx.CreateLink(0, 0));
        Assert.Equal(new[] { new Link<uint>(1, 1, 1), new Link<uint>(2, 2, 2), new Link<uint>(3, 0, 0) }, Rows(tx));
    }
}
