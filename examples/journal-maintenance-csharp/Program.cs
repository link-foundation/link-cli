using Foundation.Data.Doublets.Cli.Engine;
if (args.Length != 2) throw new ArgumentException("usage: endpoints-comma-separated fresh-prefix/");
using var store = new EtcdJournalStore(args[0].Split(',').Select(value => new Uri(value)), args[1]);
var options = new JournalScanOptions(PageKeys: 1);
void Check(bool condition) { if (!condition) throw new InvalidOperationException("Maintenance contract failed."); }
if (store.ScanNamespace(options).Count != 0) throw new InvalidOperationException("Example requires an empty dedicated namespace.");
Check(store.CompareExchange(new[] { "a", "b" }, new[] { new JournalVersion(0, Array.Empty<string>()), new(0, Array.Empty<string>()) },
    new IReadOnlyList<string>?[] { new[] { "first 雪" }, new[] { "second" } }));
var before = store.ScanNamespace(options);
Check(before.Count == 2 && before[0].Key == "a" && before[0].Version.Entries.SequenceEqual(new[] { "first 雪" }));
Check(store.CompareDelete("a", before[0].Version.Generation));
Check(!store.CompareDelete("a", before[0].Version.Generation));
Check(store.CompareExchange(new[] { "a" }, new[] { new JournalVersion(0, Array.Empty<string>()) }, new IReadOnlyList<string>?[] { new[] { "replacement" } }));
Check(!store.CompareDelete("a", before[0].Version.Generation));
var after = store.ScanNamespace(options);
Check(after[0].Version.Generation > before[0].Version.Generation && after[0].Version.Entries.SequenceEqual(new[] { "replacement" }));
Check(after[1].Version.Generation == before[1].Version.Generation && after[1].Version.Entries.SequenceEqual(before[1].Version.Entries));
var keysOnly = store.ScanNamespace(options with { KeysOnly = true });
Check(keysOnly.All(entry => entry.Version.Entries.Count == 0) && keysOnly[0].Version.Generation == after[0].Version.Generation);
Console.WriteLine("PASS C#: pinned pagination, conditional delete, absent conflict, ABA protection, keys-only scan");
