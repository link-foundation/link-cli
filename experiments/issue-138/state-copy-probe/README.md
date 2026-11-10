# Finite engine allocation control

Run from the repository root:

```sh
CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 cargo run --manifest-path rust/Cargo.toml --example engine_state_copy
```

The experiment creates 256 named rows, pins the original snapshot and performs
sixteen commits that update only row 1. Each step checks all unchanged values,
compares their allocation identities and verifies that the pinned snapshot has
not changed. Both choices run on the same API and inputs. Expected output:

```text
structure=Flat rows=256 commits=16 unchanged_row_identity_shared=0/4080
structure=Shared rows=256 commits=16 unchanged_row_identity_shared=4080/4080
```

This is a bounded identity/control experiment. It does not measure elapsed time,
whole-process allocation, serialization work or physical durability. The matching
portable regression also executes in actual WASM; C# tests inspect immutable
component node sharing. See `docs/ENGINE-SHARED-STATE.md` for remaining costs.

The runnable source is `rust/examples/engine_state_copy.rs`, using the existing
crate and dependency policy without a separate experiment manifest.
