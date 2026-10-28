# Native Rust LSM cluster consumers

From the repository root, build the optional native facade without SQLite:

```sh
cargo build --manifest-path rust/Cargo.toml --no-default-features --features native-pages,physical-lsm --example native_lsm
rust/target/debug/examples/native_lsm cluster direct
rust/target/debug/examples/native_lsm cluster partitioned
rust/target/debug/examples/native_lsm cluster chunks
rust/target/debug/examples/native_lsm cluster reclaiming
rust/target/debug/examples/native_lsm cluster reclaiming-chunks
```

Each finite consumer uses the actual `ClusterEngine<u32>` and native files in a
temporary directory. It publishes three revisions with exact Unicode link names,
rejects a stale competing session, compacts, closes every store and reopens the
same physical catalog and data stores. Reclamation also checks that obsolete
immutable objects were deleted before reopening. Temporary directories are
removed after all owners close. No server or SQLite provider is required.

The explicit `native-pages,physical-lsm` feature pair exposes
`FileLsmJournalStore` and `FileLsmOptions`. The portable physical store remains
usable with caller-owned devices independently. Keep mirror modes fixed for a
directory and close all engines/sessions before their file stores. See the
[native ownership and platform contract](../../../docs/protocol/native-lsm-store.md#rust-native-directory-selection).

The same executable also supplies finite `write`, `read`, `mutate` and
`read-advanced` commands to the [five-runtime file handoff driver](../../../examples/native-lsm-js/README.md).

The separately selected [causal Merkle example](../../../docs/ENGINE-CAUSAL-MERKLE.md)
exchanges bounded proof messages between independently owned physical stores.
Build Rust with `native-pages,physical-lsm,causal-merkle`, build the C# cluster
example, then run `python3 examples/native-lsm-js/run-merkle-interop.py` from
the repository root. It includes finite before/after publication process exits,
explicit reaped-owner recovery, read/retry hash preservation and following edits.

Select `native-pages,physical-lsm,author-fencing` to include the `fence`
subcommand. After building the C# cluster example in Release,
`NATIVE_LSM_LAYOUT=fence python3 examples/native-lsm-js/run-interop.py` checks
fifty complete-store author ownership handoffs. The [physical recipe](../../../examples/native-lsm-js/README.md)
describes the two-key publication, exact active-token retry, superseded-owner
refusal and physical hash checks. Explicit takeover does not authorize bypass
writes; see the [author guard contract](../../../docs/protocol/author-fence-v1.md).
