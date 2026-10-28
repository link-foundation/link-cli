# Native sorted journal table, version 1

A table is immutable and caller-owned on an exclusive `PageDevice`. Physical
bytes use `native-pages.md`: 4096-byte pages, doublets string cells, identity,
chained page hashes and alternating checksummed decisions. Creation starts only
on a completely empty device and commits exactly one flush. Opening never
initializes, truncates, repairs invalid logical records or appends. A selected
mirrored device may complete its existing repair only after full validation.

The selected native metadata must have generation 1. Both valid decisions must
agree on identity; duplicate generations must agree byte for byte. The committed
file has exactly the recorded pages. Torn/missing decisions are insufficient
unless a valid generation-1 decision and every referenced page validate. The
initial empty decision does not describe a sealed table. Unknown checksummed
formats/modes reject, including when an older decision is valid.

Native log record zero is exactly the string `["native-sst1"]`. Each following
record is a JSON array `[key,generation,entries]`. `key` uses the atomic journal
interface's 1..128 ASCII letters/digits/dot/dash/underscore. Records are strictly
key-ordered with no duplicates. `generation` is a positive canonical decimal
string in 1..18446744073709551615. `entries` is an array of well-formed owned
strings, or JSON null for a tombstone. A tombstone retains its generation so
later LSM composition can exclude old physical values and avoid identity reuse.
An empty record array is a live empty journal. JSON strings may escape Unicode
and delimiters; physical log records never contain raw CR/LF. Equivalent JSON
escape spellings are accepted. A marker-only file is a valid empty table.

Default admission bounds are 4096 keys and 8 MiB of total encoded UTF-8 native
record bytes, including the marker. Select 1..65536 keys and 1..64 MiB. The
corresponding maximum physical page count is derived before reading payloads.
Every input row and bound validates before creation I/O. Open validates all
native bytes and logical records before exposing a table; an excessive bound,
malformed key/generation/payload, duplicate, ordering failure or unsupported
format is an error, never partial success. No existing device is overwritten.

After validation, the table keeps sorted keys, physical record addresses and page hashes; payload strings are discarded.
One flush packs cells contiguously, so a record's end address follows its actual
UTF-8 byte length. Point lookup binary-searches the key index and verifies only
the pages containing that record against the validated hashes and identity,
then decodes the owned row at its validated address. A missing key sends
no physical page request. Returned values and complete scans are owned. A scan
error returns no partial collection. Read failures do not mutate the device.
Opening and scans perform complete bounded I/O; no claim of constant-time
startup, range streaming, compression or background compaction follows.

This format provides a physical sorted-table building block. It does not supply
WAL admission, manifest publication, memtable recovery, compaction, physical
file retirement or a complete LSM by itself. Persistence depends on the selected
page device's declared synchronization and ownership contracts. Tests of process
exits do not certify a device's behavior during physical power loss.

Rust selects `sorted-tables` independently of `etcd`, `sqlite` and native file
I/O. Add `native-pages` for a `FilePageDevice`; the browser wrapper selects the
portable table feature without a native file provider. C# uses
`SortedJournalTable`; JS imports the separate `./engine-sorted-table` entry.
The universal JS root and default Rust core do not load this table codec.
