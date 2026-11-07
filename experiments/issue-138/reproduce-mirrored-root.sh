#!/usr/bin/env bash
# Run with saved minimal fixtures in a clean isolated worktree at the baseline.
set -euo pipefail
cd "$(dirname "$0")/../.."
baseline=600cf5f0a25741c865f8730ce23d08363f76c932
if [[ "$(git rev-parse HEAD)" != "$baseline" ]] || [[ -n "$(git status --porcelain)" ]]; then
 printf '%s\n' "Requires a clean isolated worktree at $baseline; refusing to overwrite current tests." >&2
 exit 2
fi
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PATH"
mkdir -p rust/tests/support js/client/test/support
cp experiments/issue-138/mirrored-root-before.rs rust/tests/support/checkpoint_root.rs
cat >> rust/tests/engine_checkpoint_pages_tests.rs <<'RUST'

#[path = "support/checkpoint_root.rs"]
mod root_mirror;
RUST
python3 - <<'PY'
from pathlib import Path
p=Path('csharp/Foundation.Data.Doublets.Cli.Tests/CheckpointPagesTests.cs')
s=p.read_text(); assert 'public class CheckpointPagesTests' in s
p.write_text(s.replace('public class CheckpointPagesTests', 'public partial class CheckpointPagesTests', 1))
PY
cp experiments/issue-138/MirroredRootBeforeTests.cs csharp/Foundation.Data.Doublets.Cli.Tests/CheckpointRootTests.cs
cp experiments/issue-138/mirrored-root-before.test.mjs js/client/test/engine-checkpoint-root.test.mjs
cp experiments/issue-138/mirrored-root-before-support.mjs js/client/test/support/checkpoint-pages.mjs
expect_missing() {
 local language="$1" pattern="$2"
 shift 2
 if "$@" > "experiments/issue-138/mirrored-root-$language-before.log" 2>&1; then
  printf '%s\n' "Unexpected success: $language root adapter exists; inspect before continuing." >&2
  exit 1
 fi
 rg -q "$pattern" "experiments/issue-138/mirrored-root-$language-before.log"
 printf '%s\n' "$language mirrored root missing-feature control: failed as expected"
}
expect_missing rust 'error\[E0432\].*MirroredCheckpointRoot' cargo test --manifest-path rust/Cargo.toml --no-default-features --test engine_checkpoint_pages_tests mirrored_root_recovers_acknowledged
expect_missing csharp 'error CS0246:.*MirroredCheckpointRoot' dotnet test csharp --configuration Debug --disable-build-servers -m:1 --filter FullyQualifiedName~MirroredRootRecoversAcknowledged
expect_missing js 'ERR_MODULE_NOT_FOUND.*engine-checkpoint-root' node --test --test-timeout=30000 js/client/test/engine-checkpoint-root.test.mjs
