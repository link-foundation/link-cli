#!/usr/bin/env bash
# Reproduce the C# release dropping a changeset it cannot parse.
#
# Usage: repro-csharp-invalid-changeset.sh <repo> <revision>
#
# Clones <revision> of <repo> (link-cli or the csharp template) into a
# temporary directory with a bare "origin", replaces the pending changesets
# with a valid patch changeset and a minor one whose bump type has a typo
# ("minr"), and runs version-and-commit.mjs --mode changeset as the release
# job on main does. The PR check (validate-changeset.mjs) only inspects
# changesets a PR adds, so an edited changeset reaches main unchecked.
#
# Expected: the release fails and names the broken changeset. Before the fix
# it releases a patch, leaves the minor change out of CHANGELOG.md, deletes
# the broken changeset and exits 0.
set -euo pipefail

repo="$(realpath "${1:?usage: $0 <repo> <revision>}")"
revision="${2:?usage: $0 <repo> <revision>}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

git clone -q --no-local "$repo" "$work/clone"
cd "$work/clone"
git checkout -q --detach "$revision"
short="$(git rev-parse --short HEAD)"
git checkout -q -B main
git config user.name "Repro"
git config user.email "repro@example.invalid"
git init -q --bare --initial-branch=main "$work/origin.git"
git remote set-url origin "$work/origin.git"

if [ -d csharp/scripts ]; then
  scripts=csharp/scripts changesets=csharp/.changeset package=Foundation.Data.Doublets.Cli
  changelog=csharp/CHANGELOG.md
else
  scripts=scripts changesets=.changeset package=MyPackage changelog=CHANGELOG.md
fi
find "$changesets" -name '*.md' ! -name README.md -delete
printf -- "---\n'%s': patch\n---\n\nFix a typo in the help text.\n" "$package" > "$changesets/fix-help.md"
printf -- "---\n'%s': minr\n---\n\nAdd the export command.\n" "$package" > "$changesets/add-export.md"
git add -A && git commit -q -m "two pending changesets, one with a typo"
git push -q -u origin main

echo "== $(basename "$repo") $short: $scripts/version-and-commit.mjs --mode changeset"
set +e
GITHUB_OUTPUT="$work/output" node "$scripts/version-and-commit.mjs" --mode changeset > "$work/run.log" 2>&1
status=$?
set -e
cat "$work/run.log"
echo "exit=$status"
echo "== GITHUB_OUTPUT"
cat "$work/output" 2>/dev/null || echo "(not written)"
echo "== changesets left in $changesets"
ls "$changesets"
echo "== newest $changelog entry"
awk '/^## /{n++} n==1' "$changelog" 2>/dev/null | head -8 || true
echo "== does $changelog mention the minor change?"
grep -c "Add the export command" "$changelog" 2>/dev/null || true
