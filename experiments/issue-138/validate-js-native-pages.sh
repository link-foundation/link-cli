#!/usr/bin/env bash
# Sequential bounded runtime contracts. CLINK_WASM points at an existing web build.
set -euo pipefail
cd "$(dirname "$0")/../.."
: "${CLINK_WASM:?provide the generated actual WASM web package}"
(cd js/client && node --test --test-concurrency=1 --test-timeout=30000 --experimental-test-coverage --test-coverage-include='src/**' --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 test/*.test.mjs && npm run typecheck) > experiments/issue-138/paged-js-full.log 2>&1
node .github/scripts/check-sqlite-optional.mjs js > experiments/issue-138/paged-js-optional.log 2>&1
node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > experiments/issue-138/paged-repository-tests.log 2>&1
for runtime in node bun; do
 "$runtime" js/client/test/conformance/run.mjs > "experiments/issue-138/paged-$runtime-conformance.log" 2>&1
done
deno run --allow-all js/client/test/conformance/run.mjs > experiments/issue-138/paged-deno-conformance.log 2>&1
python3 experiments/issue-138/make-native-page-vectors.py experiments/issue-138/native-pages-regenerated.json
cmp docs/protocol/native-pages-v1.json experiments/issue-138/native-pages-regenerated.json
python3 experiments/issue-138/make-mirrored-page-vectors.py experiments/issue-138/mirrored-pages-regenerated.json
cmp docs/protocol/native-pages-mirrored-v1.json experiments/issue-138/mirrored-pages-regenerated.json
git diff --check
printf 'JS full coverage, types, dependency guard, repository tests, Node/Bun/Deno actual-WASM and independent vectors passed\n'
