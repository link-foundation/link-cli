# link-cli Rust Package

[![Rust CI/CD Pipeline](https://github.com/link-foundation/link-cli/actions/workflows/rust.yml/badge.svg)](https://github.com/link-foundation/link-cli/actions/workflows/rust.yml)
[![Crates.io](https://img.shields.io/crates/v/link-cli?logo=rust&label=Crates.io)](https://crates.io/crates/link-cli)
[![Docs.rs](https://docs.rs/link-cli/badge.svg)](https://docs.rs/link-cli)
[![GitHub Release](https://img.shields.io/github/v/release/link-foundation/link-cli?filter=rust-v*&label=Rust%20release)](https://github.com/link-foundation/link-cli/releases)

This directory contains the Rust `link-cli` crate, which publishes both a
reusable `[lib]` (`link_cli`) and the `clink` `[[bin]]` from the same
package. It mirrors the core query processor, named references, LiNo
import/export, and structure formatting used by the production C# tool.
The WebAssembly wrapper crate lives in `rust/wasm/` and depends on this
package.

## Install

```bash
# Build and install the CLI binary.
cargo install link-cli
```

```bash
# Or pull in the public API to build your own tooling.
cargo add link-cli
```

API documentation for every published version is hosted on
[docs.rs/link-cli](https://docs.rs/link-cli). A copy is also published to
GitHub Pages alongside the C# DocFX site by `.github/workflows/docs.yml`.

## Use

```bash
clink '() ((1 1))' --changes --after
```

### Optional Transactions and Version Control

Pass `--transactions` (or any flag in the family — `--transactions-file`,
`--commit-mode`, `--retention`, `--log`) to record each Create/Update/Delete
as a reversible transition in a sidecar doublets store. Pass `--vc`
(or `--vc-file`, `--branch`, `--branch-from`, `--checkout`, `--tag`,
`--list-branches`, `--list-tags`) to add a version-control layer over the
recorded transitions log:

```bash
# Record reversible transitions into data.transitions.links
clink --db data.links --transactions --auto-create-missing-references '() ((1 1))'
clink --db data.links --log

# Branch and tag on top of the transitions log
clink --db data.links --vc --tag v1
clink --db data.links --vc --branch feature --branch-from 1
clink --db data.links --vc --list-branches
```

End-to-end demo scripts live in
[`examples/transactions/`](../examples/transactions) and
[`examples/version-control/`](../examples/version-control).

## Develop

```bash
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features
cargo test --manifest-path rust/Cargo.toml --all-features
```

Release automation for this package lives in `rust/scripts/` and uses changelog
fragments from `rust/changelog.d/`.
