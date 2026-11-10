#!/usr/bin/env bash
# Immutable baseline replay; never replace files in the working tree.
set -euo pipefail
cd "$(dirname "$0")/../.."
baseline=c1abe42ab45392621b7b0cae131ec96fb85f5ccc
probe_dir=$(mktemp -d "$PWD/experiments/issue-138/phase-context-baseline.XXXXXX")
mkdir "$probe_dir/source" "$probe_dir/probe"
for name in FiniteExitChild FinitePhaseWatch; do
 git show "$baseline:csharp/Foundation.Data.Doublets.Cli.Tests/$name.cs" > "$probe_dir/source/$name.cs"
done
cp experiments/issue-138/phase-context-probe/{Program.cs,PhaseContextProbe.csproj} "$probe_dir/probe/"
export DOTNET_GCHeapHardLimit=0x10000000
dotnet build "$probe_dir/probe/PhaseContextProbe.csproj" --configuration Release --disable-build-servers -m:1 -p:SupervisorSourceRoot="$probe_dir/source" > "$probe_dir/build.log" 2>&1
set +e
dotnet "$probe_dir/probe/bin/Release/net10.0/PhaseContextProbe.dll" > "$probe_dir/run.log" 2>&1
result=$?
set -e
test "$result" -eq 1
rg -q 'System.TimeoutException: deferred synchronization context.*phase=5; exited 0' "$probe_dir/run.log"
rg -q 'Real finite child exited 0; queued context callbacks=[1-9]' "$probe_dir/run.log"
printf 'PASS unchanged baseline reproduces phase5/exit0 timeout; complete logs: %s\n' "$probe_dir"
