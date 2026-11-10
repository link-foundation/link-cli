#!/usr/bin/env bash
# Recheck changed concurrency contracts, then collect complete sequential evidence.
set -euo pipefail
cd "$(dirname "$0")/../.."
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PATH"
prefix=experiments/issue-138/causal-session
dotnet format csharp --no-restore --include \
  csharp/Foundation.Data.Doublets.Cli.Tests/CausalSessionTests.cs > "$prefix-csharp-format-final.log" 2>&1
bash experiments/issue-138/validate-causal-session.sh > "$prefix-full-driver.log" 2>&1
bash experiments/issue-138/validate-causal-session-actual.sh > "$prefix-actual-driver.log" 2>&1
bash experiments/issue-138/validate-causal-session-physical.sh > "$prefix-physical-driver.log" 2>&1
python3 experiments/issue-138/summarize-causal-session.py > dev/log/issues/138/pulls/139/causal-session.txt
printf 'FINAL PASS %s\n' "$(date -u +%FT%TZ)"
