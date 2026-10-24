#!/usr/bin/env bash
# Reproduce the substring exclusion false negative of the template file-size
# checks (link-cli issue #134). Each oversized file below must be reported;
# the template scripts skip them because `bin`, `obj` or `target` occurs
# somewhere in the path, including the checkout directory name.
#
# Usage: dev/log/issues/134/pulls/135/scripts/template-file-size-substring-exclusion.sh <csharp-check-file-size.mjs> <rust-check-file-size.rs>
set -uo pipefail
csharp_script="$(realpath "${1:?csharp script}")"
rust_script="$(realpath "${2:?rust script}")"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
long() { mkdir -p "$(dirname "$1")"; yes 'x' | head -n 1001 > "$1"; }

echo "=== C# template: repository checked out as 'csharp-app'"
repo="$work/csharp-app"; long "$repo/src/Robin.cs"; long "$repo/src/Objects/Store.cs"
(cd "$repo" && node "$csharp_script"); echo "exit: $?"

echo "=== C# template: repository checked out as 'robin-cli' (every path contains 'bin')"
repo="$work/robin-cli"; long "$repo/src/Program.cs"
(cd "$repo" && node "$csharp_script"); echo "exit: $?"

echo "=== Rust template: repository checked out as 'rust-app'"
repo="$work/rust-app"; long "$repo/src/retarget.rs"; long "$repo/src/target_matcher.rs"
(cd "$repo" && rust-script "$rust_script" 2>&1 | grep -v '^\s*\(Compiling\|Finished\)'); echo "exit: ${PIPESTATUS[0]}"

echo "=== Rust template: repository checked out as 'target-cli' (every path contains 'target')"
repo="$work/target-cli"; long "$repo/src/main.rs"
(cd "$repo" && rust-script "$rust_script" 2>&1 | grep -v '^\s*\(Compiling\|Finished\)'); echo "exit: ${PIPESTATUS[0]}"

echo "=== Both templates: oversized tooling under .github ('.git' is a substring of '.github')"
repo="$work/tooling"; long "$repo/.github/scripts/Tool.cs"; long "$repo/.github/scripts/tool.rs"
(cd "$repo" && node "$csharp_script"); echo "exit: $?"
(cd "$repo" && rust-script "$rust_script" 2>&1 | grep -v '^\s*\(Compiling\|Finished\)'); echo "exit: ${PIPESTATUS[0]}"
