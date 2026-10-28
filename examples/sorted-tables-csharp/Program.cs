using Foundation.Data.Doublets.Cli.Engine;
if (args.Length != 2 || args[1] is not ("create" or "read")) throw new ArgumentException("Provide <dedicated-file> create|read");
var rows = Enumerable.Range(0,80).Select(i=>new SortedJournalEntry($"k{i:000}",i==1?ulong.MaxValue:(ulong)i+1,i==1?null:i==2?Array.Empty<string>():new[]{"\ufeff雪\0😀\n"+new string('x',500)})).ToArray();
using var device = new FilePageDevice(args[0]);
var table = args[1]=="create" ? SortedJournalTable.Create(device,rows) : SortedJournalTable.Open(device);
static void Same(SortedJournalEntry expected, SortedJournalEntry actual)
{
    if (expected.Key!=actual.Key || expected.Generation!=actual.Generation || (expected.Entries is null ? actual.Entries is not null : actual.Entries is null || !expected.Entries.SequenceEqual(actual.Entries))) throw new InvalidDataException("Sorted table interoperability mismatch");
}
var all=table.Entries();if(all.Count!=rows.Length)throw new InvalidDataException("Sorted table size");
for(int i=0;i<rows.Length;i++){Same(rows[i],all[i]);Same(rows[i],table.Get(rows[i].Key)!);}
if(table.Get("missing") is not null)throw new InvalidDataException("Sorted table missing key");
Console.WriteLine($"C# native sorted table {args[1]}: {table.Count} exact rows, tombstone/empty/Unicode/u64 and indexed points; digest {table.Digest}");
