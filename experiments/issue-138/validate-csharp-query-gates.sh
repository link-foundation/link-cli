#!/usr/bin/env bash
# Small repository/release gates shared by the C# query implementation steps.
set -euo pipefail
cd "$(dirname "$0")/../.."
prefix=${VALIDATION_PREFIX:-experiments/issue-138/csharp-generic-query}
npm run test:js --prefix js > "$prefix-repository.log" 2>&1
node csharp/scripts/check-file-size.mjs > "$prefix-source-size.log" 2>&1
node csharp/scripts/validate-changeset.mjs > "$prefix-changeset.log" 2>&1
node --test csharp/scripts/release-scripts.test.mjs > "$prefix-release-tests.log" 2>&1
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck \
  experiments/issue-138/validate-csharp-query-gates.sh \
  experiments/issue-138/validate-csharp-generic-query.sh \
  experiments/issue-138/run-csharp-generic-query.sh \
  experiments/issue-138/run-csharp-query-zero.sh > "$prefix-shellcheck.log" 2>&1
git diff --check > "$prefix-whitespace.log" 2>&1
printf 'GATES PASS %s\n' "$(date -u +%FT%TZ)"
