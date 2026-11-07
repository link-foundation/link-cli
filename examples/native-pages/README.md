# Native page interoperability

Build the native examples and an actual web WASM package, then run the same
files through Rust, C#, Node, Bun and Deno:

```sh
cargo build --manifest-path rust/Cargo.toml --no-default-features --features native-pages --example engine_native_pages
dotnet build examples/native-pages-csharp
CLINK_WASM="$PWD/js/pkg" bash examples/native-pages/run-interop.sh
```

`NATIVE_PAGES_RUST_EXAMPLE` and `NATIVE_PAGES_CSHARP_EXAMPLE` can select the release
examples. The script uses a
temporary directory, removes it on exit and exercises 16 combinations of state
structure, logical encoding, reference validation and physical delta encoding.
Every invocation validates all prior revisions/rows and exact BOM/NUL/Unicode
names, retains a pinned predecessor and adds two sessions in one flush. The
final Rust invocation verifies the records written by C# and all JS runtimes.

This is an optional SQL-free physical representation; see
[native page assumptions and costs](../../docs/ENGINE-NATIVE-PAGES.md) and the
[independent wire protocol](../../docs/protocol/native-pages.md). The matching
selected redundant repair is still being developed separately.
