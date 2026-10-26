using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;
public class MaintenanceBeforeTests
{
    [Fact]
    public void NamespaceOptionsFailBeforeIo()
    {
        using var store = new EtcdJournalStore(new[] { new Uri("http://127.0.0.1:1") }, "/test/");
        IJournalMaintenanceStore maintenance = store;
        Assert.Throws<ArgumentOutOfRangeException>(() => maintenance.ScanNamespace(new JournalScanOptions(MaxKeys: 0)));
        Assert.Throws<ArgumentOutOfRangeException>(() => maintenance.CompareDelete("a", 0));
    }
}
