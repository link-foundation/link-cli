#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
prefix=experiments/issue-138/lsm-readonly-inspection
node --max-old-space-size=128 --stack-size=512 --test --test-concurrency=1 --test-timeout=30000 experiments/issue-138/inspect-lsm-wal-pages.test.mjs > "$prefix-node.log" 2>&1
bun test experiments/issue-138/inspect-lsm-wal-pages.test.mjs --smol --timeout 30000 > "$prefix-bun.log" 2>&1
deno test --allow-read --v8-flags=--max-old-space-size=128,--stack-size=512 experiments/issue-138/inspect-lsm-wal-pages.test.mjs > "$prefix-deno.log" 2>&1
printf 'PASS six read-only inspection controls in Node/Bun/Deno %s\n' "$(date -u +%FT%TZ)"
