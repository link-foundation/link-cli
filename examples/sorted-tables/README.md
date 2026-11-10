# Immutable native sorted tables

This example creates a physical sealed table of 80 sorted journal keys, then
opens every writer's exact files in Rust, C#, Node, Bun and Deno. Every reader
checks a complete scan and all point reads, including a u64-max tombstone,
a live empty journal and Unicode/BOM/NUL/newline values. The matrix checks all
25 writer/reader pairs, hashes all three files before and after each reader,
and checks that each exclusive owner released its lock.

Build from the repository root with Rust, .NET 10, Node, Bun and Deno available:

```sh
cargo build --manifest-path rust/Cargo.toml --no-default-features --features native-pages,sorted-tables --example sorted_table
dotnet build examples/sorted-tables-csharp
bash examples/sorted-tables/run-interop.sh
```

The script uses dedicated temporary files and removes them on exit. The Rust
`sorted-tables` feature selects JSON table records independently of `etcd`,
`sqlite` and native file I/O. The portable device API runs in actual WASM tests;
the native file matrix needs operating-system files. C# and JS select the table
class explicitly; JS exposes it only through `./engine-sorted-table`.

See [wire format, bounds, ownership and costs](../../docs/protocol/native-sorted-table.md).
This is an immutable table building block. WAL/memtable/manifest publication,
compaction and obsolete-file retirement remain separate LSM composition work.
Process/file tests do not certify physical power-loss behavior.
