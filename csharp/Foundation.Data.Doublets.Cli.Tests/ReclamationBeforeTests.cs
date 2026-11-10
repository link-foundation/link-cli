using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class ReclamationBeforeTests
{
    [Fact]
    public void InvalidReclamationBudgetRejectsBeforeNetworkIo()
    {
        using var catalog = new EtcdJournalStore(new[] { new Uri("http://127.0.0.1:1") }, "/catalog/");
        using var data = new EtcdJournalStore(new[] { new Uri("http://127.0.0.1:1") }, "/data/");
        Assert.Throws<ArgumentOutOfRangeException>(() => ReclaimingPartitionedJournalStore.Open(
            catalog, new Dictionary<string, IJournalMaintenanceStore> { ["east"] = data },
            _ => "east", new ReclamationOptions(MaxKeys: 0)));
    }
}
