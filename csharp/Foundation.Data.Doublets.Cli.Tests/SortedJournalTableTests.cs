using System.Text;
using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;
namespace Foundation.Data.Doublets.Cli.Tests;

public class SortedJournalTableTests
{
    private static SortedJournalEntry[] Rows() => new[] {
        new SortedJournalEntry("a",1,new[]{"\ufeff雪\0😀\n"}),new SortedJournalEntry("b",ulong.MaxValue,null),new SortedJournalEntry("c",3,Array.Empty<string>()) };
    private static void EqualRows(IReadOnlyList<SortedJournalEntry> expected, IReadOnlyList<SortedJournalEntry> actual)
    {
        Assert.Equal(expected.Count, actual.Count);
        for (int i = 0; i < expected.Count; i++) { Assert.Equal(expected[i].Key, actual[i].Key); Assert.Equal(expected[i].Generation, actual[i].Generation); Assert.Equal(expected[i].Entries, actual[i].Entries); }
    }
    [Fact]
    public void SealedTableRoundtripTombstonesAndOwnedValues()
    {
        var device = new PagedTestDevice(); var table = SortedJournalTable.Create(device, Rows());
        Assert.Equal(3, table.Count); Assert.True(table.SupportsDurableFlush);
        foreach (var row in Rows()) { var actual = table.Get(row.Key)!; Assert.Equal(row.Generation, actual.Generation); Assert.Equal(row.Entries, actual.Entries); }
        Assert.Null(table.Get("absent")); EqualRows(Rows(), table.Entries());
        ((string[])table.Get("a")!.Entries!)[0] = "caller"; Assert.Equal(Rows()[0].Entries, table.Get("a")!.Entries);
        var reopened = SortedJournalTable.Open(device.Crash()); EqualRows(Rows(), reopened.Entries());
    }
    [Fact]
    public void IndependentVectorsValidateBeforePublicationWithoutMutation()
    {
        using var json = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol/native-sorted-table-v1.json")));
        foreach (var fixture in json.RootElement.GetProperty("cases").EnumerateArray())
        {
            var image = new PagedTestDevice.Image
            {
                Metadata = fixture.GetProperty("metadata").EnumerateArray().Select(p => p.GetString() is "" ? null : Convert.FromHexString(p.GetString()!)).ToArray(),
                Pages = fixture.GetProperty("pages").EnumerateArray().Select(p => Convert.FromHexString(p.GetString()!)).ToList()
            };
            var device = PagedTestDevice.From(image);
            if (fixture.GetProperty("valid").GetBoolean())
            {
                var table = SortedJournalTable.Open(device);
                var expected = fixture.GetProperty("rows").EnumerateArray().Select(r => new SortedJournalEntry(r[0].GetString()!, ulong.Parse(r[1].GetString()!), r[2].ValueKind == JsonValueKind.Null ? null : r[2].EnumerateArray().Select(v => v.GetString()!).ToArray())).ToArray();
                EqualRows(expected, table.Entries()); Assert.Equal(fixture.GetProperty("records").EnumerateArray().Sum(r => Encoding.UTF8.GetByteCount(r.GetString()!)), table.StoredBytes);
                Assert.Equal(Enumerable.Range(0, 16).Select(i => (byte)i), table.Identity); Assert.NotEqual(0UL, table.Digest);
            }
            else Assert.NotNull(Record.Exception(() => SortedJournalTable.Open(device)));
            PagedTestDevice.Same(image, device.Live); PagedTestDevice.Same(image, device.Durable); Assert.All(device.Calls, call => Assert.Equal("readPage", call));
        }
    }
    [Fact]
    public void InvalidRowsAndBoundsPerformNoIo()
    {
        var invalid = new[] {new[]{Rows()[1],Rows()[0]},new[]{Rows()[0],Rows()[0]},new[]{Rows()[0] with {Generation=0}},
            new[]{Rows()[0] with {Key=""}},new[]{Rows()[0] with {Key="a/b"}},new[]{Rows()[0] with {Key=new string('a',129)}},
            new[]{Rows()[0] with {Entries=new[]{"\ud800"}}},new[]{Rows()[0] with {Entries=new string[]{null!}}},new SortedJournalEntry[]{null!} };
        foreach (var input in invalid) { var device = new PagedTestDevice(); Assert.NotNull(Record.Exception(() => SortedJournalTable.Create(device, input))); Assert.Equal(0, device.Reads); Assert.Empty(device.Calls); }
        foreach (var bounds in new[] { new SortedTableOptions(0), new SortedTableOptions(65537), new SortedTableOptions(MaxBytes: 0), new SortedTableOptions(MaxBytes: 67108865), new SortedTableOptions(2), new SortedTableOptions(MaxBytes: 1) })
        { var device = new PagedTestDevice(); Assert.NotNull(Record.Exception(() => SortedJournalTable.Create(device, Rows(), bounds))); Assert.Equal(0, device.Reads); Assert.Empty(device.Calls); }
        var empty = new PagedTestDevice(); Assert.Throws<ArgumentNullException>(() => SortedJournalTable.Create(empty, null!)); Assert.ThrowsAny<Exception>(() => SortedJournalTable.Create(empty, Array.Empty<SortedJournalEntry>(), new(MaxBytes: 1))); Assert.Equal(0, empty.Reads);
    }
    [Fact]
    public void PointLookupReadsSelectedPagesAndDetectsPostOpenDamage()
    {
        var rows = Enumerable.Range(0, 80).Select(i => new SortedJournalEntry($"k{i:000}", (ulong)i + 1, new[] { new string('x', 1000) })).ToArray();
        var device = new PagedTestDevice(); var table = SortedJournalTable.Create(device, rows); Assert.True(device.Live.Pages.Count > 40);
        device.Calls.Clear(); Assert.Equal(rows[40].Entries, table.Get("k040")!.Entries); Assert.InRange(device.Calls.Count, 1, 3);
        device.Calls.Clear(); Assert.Null(table.Get("j")); Assert.Null(table.Get("k999")); Assert.Empty(device.Calls);
        var identity = table.Identity; identity[0] ^= 1; Assert.Equal(rows[0].Entries, table.Get("k000")!.Entries);
        Assert.Throws<ArgumentException>(() => table.Get("bad/name")); device.SupportsDurableFlush = false; Assert.False(table.SupportsDurableFlush);
        device.ReadError = true; Assert.Throws<IOException>(() => table.Get("k040")); device.ReadError = false; Assert.Equal(rows[40].Entries, table.Get("k040")!.Entries);
        device.Live.Pages[^1][64] ^= 1; Assert.ThrowsAny<Exception>(() => table.Get("k079")); Assert.ThrowsAny<Exception>(() => table.Entries());
    }
    [Fact]
    public void FiniteCreationCutsNeverExposePartialSealedTable()
    {
        var probe = new PagedTestDevice(); SortedJournalTable.Create(probe, Rows());
        for (int i = 0; i < probe.Calls.Count; i++)
        {
            if (probe.Calls[i] == "readPage") continue; foreach (bool after in new[] { false, true })
            {
                var device = new PagedTestDevice { Fault = (i + 1, after) }; Assert.Throws<IOException>(() => SortedJournalTable.Create(device, Rows()));
                var recovered = device.Crash(); var before = recovered.Live.Copy(); SortedJournalTable? table = null;
                var failure = Record.Exception(() => table = SortedJournalTable.Open(recovered));
                if (failure is null) EqualRows(Rows(), table!.Entries()); else Assert.IsType<IOException>(failure);
                PagedTestDevice.Same(before, recovered.Live);
            }
        }
    }
    [Fact]
    public void TornSealTrailingPagesAndExistingDataNeverMutateStorage()
    {
        var device = new PagedTestDevice(); SortedJournalTable.Create(device, Rows()); var good = device.Live.Copy();
        Assert.Throws<IOException>(() => SortedJournalTable.Create(device, Rows())); PagedTestDevice.Same(good, device.Live);
        foreach (var bounds in new[] { new SortedTableOptions(MaxBytes: 8), new SortedTableOptions(1), new SortedTableOptions(1, 300) }) Assert.ThrowsAny<Exception>(() => SortedJournalTable.Open(PagedTestDevice.From(good), bounds));
        for (int length = 0; length < 128; length++) { var image = good.Copy(); image.Metadata[1] = image.Metadata[1]![..length]; var bad = PagedTestDevice.From(image); Assert.Throws<IOException>(() => SortedJournalTable.Open(bad)); PagedTestDevice.Same(image, bad.Live); }
        var trailing = good.Copy(); trailing.Pages.Add(new byte[4096]); var invalid = PagedTestDevice.From(trailing); Assert.Throws<IOException>(() => SortedJournalTable.Open(invalid)); PagedTestDevice.Same(trailing, invalid.Live);
        Assert.Throws<IOException>(() => SortedJournalTable.Open(new PagedTestDevice()));
    }

    [Fact]
    public void ConflictingDecisionsRejectBeforeRepairWhileIdenticalSealsValidate()
    {
        var probe = new PagedTestDevice(); SortedJournalTable.Create(probe, Rows()); var good = probe.Live.Copy();
        foreach (bool sameGeneration in new[] { false, true })
        {
            var image = good.Copy();
            if (sameGeneration) { image.Metadata[0] = image.Metadata[1]!.ToArray(); image.Metadata[0]![72] ^= 1; } else image.Metadata[0]![16] ^= 1;
            PagedTestDevice.Sign(image.Metadata[0]!); var device = PagedTestDevice.From(image);
            Assert.Throws<IOException>(() => SortedJournalTable.Open(device)); Assert.Equal(0, device.Completed); Assert.Empty(device.Calls); PagedTestDevice.Same(image, device.Live);
        }
        var identical = good.Copy(); identical.Metadata[0] = identical.Metadata[1]!.ToArray(); var valid = PagedTestDevice.From(identical); EqualRows(Rows(), SortedJournalTable.Open(valid).Entries()); Assert.Equal(1, valid.Completed);
        var mode = PagedTestDevice.From(good); mode.MetadataFlags = 2; Assert.Throws<IOException>(() => SortedJournalTable.Open(mode)); Assert.Equal(0, mode.Reads);
    }
    [Fact]
    public void SelectedMirroredTableRepairsOnlyAfterCompleteValidation()
    {
        var first = new PagedTestDevice(); var second = new PagedTestDevice();
        SortedJournalTable.Create(new MirroredPageDevice(first, second), Rows());
        first.Live.Pages[0][64] ^= 1; first.Live.Metadata[1] = first.Live.Metadata[1]![..17];
        var table = SortedJournalTable.Open(new MirroredPageDevice(first, second)); EqualRows(Rows(), table.Entries()); PagedTestDevice.Same(second.Live, first.Live);
        first.Live.Pages[0][64] ^= 1; second.Live.Pages[0][64] ^= 2; var a = first.Live.Copy(); var b = second.Live.Copy();
        Assert.Throws<IOException>(() => SortedJournalTable.Open(new MirroredPageDevice(first, second))); PagedTestDevice.Same(a, first.Live); PagedTestDevice.Same(b, second.Live);
    }
}
