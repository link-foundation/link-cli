#!/usr/bin/env bash
# Repository checks can be rerun after relocating local, ignored CI evidence.
set -euo pipefail
cd "$(dirname "$0")/../.."
prefix=experiments/issue-138/reclamation-admission
printf 'REPOSITORY CHECKS %s\n' "$(date -u +%FT%TZ)"
npm run test:js --prefix js > "$prefix-repository.log" 2>&1
node csharp/scripts/check-file-size.mjs > "$prefix-source-sizes.log" 2>&1
node csharp/scripts/validate-changeset.mjs > "$prefix-changeset.log" 2>&1
GITHUB_TOKEN="$(gh auth token)" node .github/scripts/check-latest-dependencies.mjs > "$prefix-dependency-freshness.log" 2>&1
PATH="$PWD/experiments/issue-138/tools/shellcheck-v0.11.0:$PATH" experiments/issue-138/tools/actionlint > "$prefix-workflows.log" 2>&1
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck experiments/issue-138/{validate-admission-repository,validate-reclamation-admission}.sh >> "$prefix-workflows.log" 2>&1
git diff --check > "$prefix-whitespace.log" 2>&1
printf 'ALL PASS %s\n' "$(date -u +%FT%TZ)"
