# Native JavaScript LSM cluster consumers

Build the current WASM package with `wasm-pack build --target web --out-dir
../../js/pkg rust/wasm`, then run from the repository root:

```sh
CLINK_WASM=js/pkg node examples/native-lsm-js/cluster.mjs
CLINK_WASM=js/pkg bun examples/native-lsm-js/cluster.mjs
CLINK_WASM=js/pkg deno run --allow-all examples/native-lsm-js/cluster.mjs
```

The example uses the actual WASM links engine and actual native files in temporary
directories. It checks three finite compositions: a direct journal store, an
atomic catalog over two data stores, and epoch-fenced reclamation over the same
layout. Each runs three coordinated revisions with retained-history checkpoint
journals, rejects a stale group after a competing commit, compacts, closes and
reopens the physical stores, then reads the original link names. The reclamation
case also deletes obsolete immutable objects before reopening.

The file store is imported separately from
`@link-foundation/link-cli-client/engine-file-lsm-store`. Create its dedicated
parent directory first. The example closes every transaction before its stores
and removes temporary directories on completion. It does not require an external
coordinator or SQLite. See the [native ownership, finite bounds and platform
contract](../../docs/protocol/native-lsm-store.md#javascript-native-directory-selection).

Rust/C# native facade parity and whole-store physical interchange remain required
work in issue #138. This example verifies real JavaScript cluster consumers with
the linked WASM engine; it does not establish cross-language store parity.
