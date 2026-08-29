#!/usr/bin/env bash
# Reproduces the platform-data 2.0.0 external-range overlap.
# Exits 0 while the overlap reproduces, and non-zero once upstream fixes it.
set -u
cd "$(dirname "$0")"
cargo run --quiet
