#!/usr/bin/env bash
# Reproduce the Rust release false negative of issue #134 in a throwaway repo.
#
# Usage: repro-rust-release-commit.sh <git-rev-of-version-and-commit.rs | WORKTREE>
#   e.g. repro-rust-release-commit.sh e53813e   # before the fix
#        repro-rust-release-commit.sh WORKTREE  # current working tree
#
# The sandbox copies rust/Cargo.toml, rust/Cargo.lock, rust/changelog.d and
# rust/scripts from the repository (no rust/CHANGELOG.md, exactly as on main),
# pushes them to a local bare "origin", runs version-and-commit.rs and prints
# what reached the remote. Needs rust-script and network access to crates.io.
set -euo pipefail

rev="${1:?usage: $0 <rev|WORKTREE>}"
repo_root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
sandbox="$(mktemp -d)"
trap 'rm -rf "$sandbox"' EXIT

export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=init.defaultBranch GIT_CONFIG_VALUE_0=main
git init --quiet --bare "$sandbox/origin.git"
git clone --quiet "$sandbox/origin.git" "$sandbox/work" 2>/dev/null
mkdir -p "$sandbox/work/rust"
cp "$repo_root/rust/Cargo.toml" "$repo_root/rust/Cargo.lock" "$sandbox/work/rust/"
cp -R "$repo_root/rust/changelog.d" "$repo_root/rust/scripts" "$sandbox/work/rust/"
rm -f "$sandbox/work/rust/CHANGELOG.md"
if [ "$rev" != WORKTREE ]; then
  git -C "$repo_root" show "$rev:rust/scripts/version-and-commit.rs" \
    > "$sandbox/work/rust/scripts/version-and-commit.rs"
fi

cd "$sandbox/work"
git config user.name repro
git config user.email repro@example.com
git add -A
git commit --quiet -m "state of main"
git push --quiet -u origin main

fragments_before=$(find rust/changelog.d -name '*.md' ! -name README.md | wc -l)
set +e
GITHUB_OUTPUT="$sandbox/output" rust-script rust/scripts/version-and-commit.rs \
  --rust-root rust --tag-prefix rust-v --bump-type patch > "$sandbox/run.log" 2>&1
status=$?
set -e
grep -v '^\s*\(Compiling\|Downloaded\|Downloading\|Updating\|Locking\|Adding\|Finished\)' "$sandbox/run.log" | tail -25

echo "--- exit status: $status"
echo "--- GITHUB_OUTPUT:"; cat "$sandbox/output" 2>/dev/null || true
echo "--- fragments before: $fragments_before, left in work tree: $(find rust/changelog.d -name '*.md' ! -name README.md | wc -l)"
echo "--- origin/main log:"; git --git-dir="$sandbox/origin.git" log --oneline main
echo "--- origin tags:"; git --git-dir="$sandbox/origin.git" tag --list
echo "--- version on origin/main: $(git --git-dir="$sandbox/origin.git" show main:rust/Cargo.toml | grep -m1 '^version')"
echo "--- uncommitted changes left in the work tree:"; git status --short | head -5
