#!/usr/bin/env bash
# Replay the proven dependency against immutable source and isolated artifacts.
set -euo pipefail
cd "$(dirname "$0")/../.."
baseline=89fbfadf145dab8e5671838f47752d2e047ed7c9
probe_dir=$(mktemp -d "$PWD/experiments/issue-138/phase-pool-baseline.XXXXXX")
mkdir "$probe_dir/source" "$probe_dir/probe"
for name in FiniteExitChild FinitePhaseWatch; do
 git show "$baseline:csharp/Foundation.Data.Doublets.Cli.Tests/$name.cs" > "$probe_dir/source/$name.cs"
done
cp experiments/issue-138/phase-pool-probe/{Program.cs,PhasePoolProbe.csproj} "$probe_dir/probe/"
export DOTNET_GCHeapHardLimit=0x10000000
dotnet build "$probe_dir/probe/PhasePoolProbe.csproj" --configuration Release --disable-build-servers -m:1 -p:SupervisorSourceRoot="$probe_dir/source" > "$probe_dir/build.log" 2>&1
set +e
dotnet "$probe_dir/probe/bin/Release/net10.0/PhasePoolProbe.dll" > "$probe_dir/run.log" 2>&1
result=$?
set -e
test "$result" -eq 1
rg -q 'System.TimeoutException: bounded shared pool control.*phase=5; exited 0' "$probe_dir/run.log"
rg -q 'Finite child completed all phases; shared worker still deliberately occupied' "$probe_dir/run.log"
printf 'PASS immutable baseline reproduces occupied-pool phase5/exit0 timeout; complete logs: %s\n' "$probe_dir"
