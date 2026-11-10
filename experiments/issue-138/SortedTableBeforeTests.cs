using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;
public class SortedJournalTableTests
{
    private static SortedJournalEntry[] Rows() => new[] {
        new SortedJournalEntry("a",1,new[]{"\ufeff雪\0😀\n"}),new SortedJournalEntry("b",ulong.MaxValue,null),new SortedJournalEntry("c",3,Array.Empty<string>()) };
    [Fact]
    public void SealedTableRoundtripTombstonesAndOwnedValues()
    {
        var device=new PagedTestDevice();var table=SortedJournalTable.Create(device,Rows());
        Assert.Equal(3,table.Count);Assert.True(table.SupportsDurableFlush);
        foreach(var row in Rows()){var actual=table.Get(row.Key)!;Assert.Equal(row.Generation,actual.Generation);Assert.Equal(row.Entries,actual.Entries);}
        Assert.Null(table.Get("absent"));Assert.Equal(3,table.Entries().Count);
        var reopened=SortedJournalTable.Open(device.Crash());Assert.Equal(Rows()[0].Entries,reopened.Get("a")!.Entries);
    }
}
