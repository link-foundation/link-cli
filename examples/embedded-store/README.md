# Embedded store — examples

Runnable demonstrations of using the libraries **as an embedded
transactional store** instead of through the `clink` CLI, added for issue
[#98](https://github.com/link-foundation/link-cli/issues/98).

Every other folder under `examples/` drives the CLI binary. These two
programs never start `clink`: they link against the library and open a
store directly, the way a consuming application would.

## Scripts

| File | What it shows |
|------|---------------|
| `run-rust.sh`  | Embeds `link_cli` — a `usize`-addressed, file-mapped `doublets::unit::Store` behind the transactions layer |
| `run-csharp.sh` | Embeds `Foundation.Data.Doublets.Cli.Library` — a `ulong`-addressed store behind the transactions layer |
| `csharp/` | Source of the C# program (`dotnet run --project examples/embedded-store/csharp`) |
| `README.md` | This file |

The Rust program lives in the library crate itself, as
[`rust/examples/embedded_store.rs`](../../rust/examples/embedded_store.rs),
so CI compiles it on every change and the documented API cannot silently
rot. Run it directly with:

```bash
cargo run --manifest-path rust/Cargo.toml --example embedded_store
```

## What the demos do

Both walk the same four steps, one per property that issue #98 asked for:

1. **Commit one write, abandon another.** A transaction is opened and the
   handle is dropped without committing — which is what a crash looks
   like to the next process that opens the store.
2. **Reopen.** Recovery runs in the constructor: the committed write is
   still there, and the write from the transaction that never committed
   has been rolled back. No explicit save was needed for the committed
   one to survive.
3. **Lock out a second writer.** While the first holder is open, an
   attempt to take the exclusive lock fails rather than corrupting the
   database. The Rust program additionally asserts that the database
   file's **inode is unchanged**, proving it was mutated in place and
   that another process's mapping of the same file is still valid.
4. **Notice somebody else's write.** A `StorageRevision` fingerprint
   taken before an external commit reports the change afterwards,
   without reparsing the database.

## Expected output

```text
committed link 1
wrote link 2 inside a transaction that never commits
a second writer is locked out while the first one is open
after recovery: 1 is present, 2 is gone
inode unchanged (…): another process's mapping stays valid
another holder committed link 2
revision changed: true
links in the store: 2
```

The C# program prints the same story with .NET spelling (`True`/`False`)
and without the inode line, since `LinksFileLock` guards the sidecar
rather than the mapping.

## Related documentation

- [`rust/README.md` § Use as a library](../../rust/README.md#use-as-a-library)
- [`csharp/README.md` § Use as a library](../../csharp/README.md#use-as-a-library)
- [`docs/ARCHITECTURE.md` § Embedding the Library](../../docs/ARCHITECTURE.md#embedding-the-library)
- [`docs/case-studies/issue-98/README.md`](../../docs/case-studies/issue-98/README.md)
