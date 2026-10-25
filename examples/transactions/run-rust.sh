#!/usr/bin/env bash
# Demonstrates the optional transactions layer with the Rust `clink` binary.
#
# Usage:
#   ./examples/transactions/run-rust.sh
#
# Builds and runs the binary from the rust/ workspace. All artifacts are
# written into a fresh tmp directory so multiple runs do not pollute each
# other.

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

clink() {
    cargo run --manifest-path "$repo_root/rust/Cargo.toml" --quiet -- "$@"
}

cd "$work_dir"

echo "=== 1. Create two links with --transactions (sync, default retention) ==="
clink --db data.links --transactions "() ((1 1) (2 2))"
ls -1 *.links

echo
echo "=== 2. Inspect the transitions log ==="
clink --db data.links --log

echo
echo "=== 3. Create another link with explicit async commits and sized retention ==="
clink \
    --db data.links \
    --commit-mode async \
    --retention sized:128 \
    "() ((3 3))"

echo
echo "=== 4. Print the log again — sequence numbers grew ==="
clink --db data.links --log

echo
echo "=== 5. Try a chunked retention archive (every 1 transition rolls over) ==="
mkdir -p "$work_dir/archive"
clink \
    --db data.links \
    --retention "chunked:1:$work_dir/archive" \
    "() ((4 4))"
ls -1 "$work_dir/archive" || true

echo
echo "=== 6. Share archive storage with an independent database ==="
shopt -s nullglob
previous_archives=("$work_dir/archive/"*.log)
(( ${#previous_archives[@]} > 0 ))
mkdir "$work_dir/archive-before"
cp "${previous_archives[@]}" "$work_dir/archive-before/"
clink \
    --db other.links \
    --retention "chunked:1:$work_dir/archive" \
    "() ((1 1))"
current_archives=("$work_dir/archive/"*.log)
(( ${#current_archives[@]} > ${#previous_archives[@]} ))
for archive in "${previous_archives[@]}"; do
    cmp "$archive" "$work_dir/archive-before/${archive##*/}"
done
echo "Both databases have archives; every previous archive is unchanged."

echo
echo "Demo complete. Working dir: $work_dir"
