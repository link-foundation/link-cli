#!/usr/bin/env bash
# Exercise the existing release-script tests without building the database.
set -euo pipefail
cd "$(dirname "$0")"

# rust-script uses these dependencies too. TLS is unnecessary for the mocked
# registry tests. Generate ignored Cargo metadata beside this reusable probe.
cat > Cargo.toml <<'TOML'
[package]
name = "issue-140-release-lockfiles"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
path = "../../../rust/scripts/version-and-commit.rs"

[dependencies]
regex = "1"
chrono = "0.4"
ureq = { version = "2", default-features = false }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
TOML

export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo test --manifest-path Cargo.toml --lib "$@"
