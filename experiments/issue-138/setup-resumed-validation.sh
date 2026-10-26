#!/usr/bin/env bash
# Linux x86_64 tool setup; requires gh, rustup, tar and sha256sum.
set -euo pipefail
cd "$(dirname "$0")/../.."
tools_dir="$PWD/experiments/issue-138/tools"
mkdir -p "$tools_dir"
gh release download v0.9.1 --repo taiki-e/cargo-llvm-cov --pattern cargo-llvm-cov-x86_64-unknown-linux-gnu.tar.gz --dir "$tools_dir" --clobber
tar -xzf "$tools_dir/cargo-llvm-cov-x86_64-unknown-linux-gnu.tar.gz" -C "$tools_dir"
gh release download 0.2.129 --repo wasm-bindgen/wasm-bindgen --pattern wasm-bindgen-0.2.129-x86_64-unknown-linux-musl.tar.gz --pattern wasm-bindgen-0.2.129-x86_64-unknown-linux-musl.tar.gz.sha256sum --dir "$tools_dir" --clobber
(cd "$tools_dir" && sed 's#gh-release/##' wasm-bindgen-0.2.129-x86_64-unknown-linux-musl.tar.gz.sha256sum | sha256sum -c -)
tar -xzf "$tools_dir/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl.tar.gz" -C "$tools_dir"
gh release download v1.7.12 --repo rhysd/actionlint --pattern actionlint_1.7.12_linux_amd64.tar.gz --dir "$tools_dir" --clobber
tar -xzf "$tools_dir/actionlint_1.7.12_linux_amd64.tar.gz" -C "$tools_dir" actionlint
gh release download v0.11.0 --repo koalaman/shellcheck --pattern shellcheck-v0.11.0.linux.x86_64.tar.xz --dir "$tools_dir" --clobber
tar -xJf "$tools_dir/shellcheck-v0.11.0.linux.x86_64.tar.xz" -C "$tools_dir"
rustup component add llvm-tools-preview
rustup target add wasm32-unknown-unknown
rustup toolchain install 1.98.0 --profile minimal --no-self-update --component llvm-tools-preview --target wasm32-unknown-unknown
