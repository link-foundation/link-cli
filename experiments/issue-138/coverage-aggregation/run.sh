#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
coverage_sysroot=$(rustc --print sysroot)
coverage_host=$(rustc -vV | sed -n 's/^host: //p')
coverage_tools="$coverage_sysroot/lib/rustlib/$coverage_host/bin"
rustc -vV > versions.txt
RUSTC_BOOTSTRAP=1 rustc -Cinstrument-coverage -Zcoverage-options=branch -Cdebuginfo=0 main.rs -o probe
LLVM_PROFILE_FILE=probe.profraw ./probe
"$coverage_tools/llvm-profdata" merge -sparse probe.profraw -o probe.profdata
"$coverage_tools/llvm-cov" export -instr-profile=probe.profdata ./probe > coverage.json
"$coverage_tools/llvm-cov" show -instr-profile=probe.profdata ./probe -show-instantiations -show-branches=count > source.txt
"$coverage_tools/llvm-cov" report -instr-profile=probe.profdata ./probe > summary.txt
cat summary.txt
