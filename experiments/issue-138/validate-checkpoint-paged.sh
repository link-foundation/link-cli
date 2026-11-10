#!/usr/bin/env bash
# The package was built by validate-checkpoint-delta.sh wasm; keep compilers sequential.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 DOTNET_GCHeapHardLimit=0x28000000
stage="${1:?choose examples, interop or repository}"
case "$stage" in
 examples)
  cargo fmt --manifest-path rust/Cargo.toml --all -- --check
  cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > experiments/issue-138/checkpoint-paged-clippy.log 2>&1
  cargo build --manifest-path rust/Cargo.toml --no-default-features --features native-pages --example engine_native_pages > experiments/issue-138/checkpoint-paged-rust-build.log 2>&1
  dotnet format examples/native-pages-csharp --verify-no-changes > experiments/issue-138/checkpoint-paged-csharp-format.log 2>&1
  dotnet build examples/native-pages-csharp --configuration Debug --disable-build-servers -m:1 > experiments/issue-138/checkpoint-paged-csharp-build.log 2>&1
  export CLINK_WASM="${CLINK_WASM:-$PWD/experiments/issue-138/checkpoint-delta-wasm-package}"
  python3 experiments/issue-138/verify-checkpoint-paged-entrypoints.py > experiments/issue-138/checkpoint-paged-entrypoints.log 2>&1
  ;;
 interop)
  export CLINK_WASM="${CLINK_WASM:-$PWD/experiments/issue-138/checkpoint-delta-wasm-package}"
  bash examples/journal-checkpoints/run-paged-interop.sh > experiments/issue-138/checkpoint-paged-validation.log 2>&1
  bash examples/native-pages/run-interop.sh > experiments/issue-138/checkpoint-paged-ordinary-control.log 2>&1
  python3 - <<'PY'
import re
from pathlib import Path
text = Path('experiments/issue-138/checkpoint-paged-ordinary-control.log').read_text()
configurations = re.findall(r'^(single|mirrored)/(scan|indexed)/(flat|shared|sorted-runs)/(snapshot|changes)/(plain|delta)$', text, re.M)
assert len(configurations) == len(set(configurations)) == 48
groups = re.findall(r'^Native pages group: (\d+) -> (\d+) revisions/rows;', text, re.M)
assert groups == [(str(writer * 2), str(2 + writer * 2)) for _ in range(48) for writer in range(6)]
print('PASS ordinary pages control: 48 configurations, 288 persistent writer handoffs, revision/rows 0 -> 12.')
PY
  ;;
 repository)
  python3 experiments/issue-138/generate-checkpoint-vectors.py experiments/issue-138/checkpoint-paged-regenerated.json
  cmp docs/protocol/engine-checkpoint-vectors.json experiments/issue-138/checkpoint-paged-regenerated.json
  node csharp/scripts/validate-changeset.mjs > experiments/issue-138/checkpoint-paged-changeset.log 2>&1
  node csharp/scripts/check-file-size.mjs --lang all > experiments/issue-138/checkpoint-paged-file-size.log 2>&1
  for language in rust csharp js; do
   node .github/scripts/check-sqlite-optional.mjs "$language" > "experiments/issue-138/checkpoint-paged-$language-optional.log" 2>&1
  done
  node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > experiments/issue-138/checkpoint-paged-repository.log 2>&1
  "$PWD/experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck" examples/native-pages/run-interop.sh examples/journal-checkpoints/run-paged-interop.sh experiments/issue-138/validate-checkpoint-paged.sh > experiments/issue-138/checkpoint-paged-shellcheck.log 2>&1
  "$PWD/experiments/issue-138/tools/actionlint" -shellcheck="$PWD/experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck" .github/workflows/js-client.yml > experiments/issue-138/checkpoint-paged-actionlint.log 2>&1
  git diff --check
  ;;
 *) printf 'Unknown stage: %s\n' "$stage" >&2; exit 2 ;;
esac
printf 'Checkpoint-paged %s stage: passed\n' "$stage"
