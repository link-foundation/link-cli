// Install in the normal C# test project after the raw-session validation.
using Foundation.Data.Doublets.Cli.Engine;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests;

public class EngineQueryAllocationTests
{
    [Fact]
    public void AnonymousForwardReferenceDoesNotConsumeItsPredictedIdentity()
    {
        var engine = new LinksEngine<uint>(new MemoryCommitLog());
        using var tx = engine.Begin();
        AdvancedMixedQueryProcessor.ProcessQuery(tx, new()
        {
            Query = "(() ((1 1)))",
            AutoCreateMissingReferences = false
        });
        Assert.Equal(new Link<uint>(1, 1, 1), tx.GetLink(1));
        Assert.Equal(1u, tx.Count(null));
        Assert.Empty(engine.Snapshot().Rows);
        tx.Commit();
        Assert.Equal(new Link<uint>(1, 1, 1), Assert.Single(engine.Snapshot().Rows).Value.Link);
    }
}
