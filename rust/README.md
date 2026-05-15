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

## Develop

```bash
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features
cargo test --manifest-path rust/Cargo.toml --all-features
```

Release automation for this package lives in `rust/scripts/` and uses changelog
fragments from `rust/changelog.d/`.
