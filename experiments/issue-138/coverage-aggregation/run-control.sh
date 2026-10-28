#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
coverage_sysroot=$(rustc --print sysroot)
coverage_host=$(rustc -vV | sed -n 's/^host: //p')
coverage_tools="$coverage_sysroot/lib/rustlib/$coverage_host/bin"
rustc -vV > control-versions.txt
RUSTC_BOOTSTRAP=1 rustc -Cinstrument-coverage -Zcoverage-options=branch -Cdebuginfo=0 control.rs -o control
LLVM_PROFILE_FILE=control.profraw ./control
"$coverage_tools/llvm-profdata" merge -sparse control.profraw -o control.profdata
"$coverage_tools/llvm-cov" export -instr-profile=control.profdata ./control > control-coverage.json
"$coverage_tools/llvm-cov" show -instr-profile=control.profdata ./control -show-instantiations -show-branches=count > control-source.txt
"$coverage_tools/llvm-cov" report -instr-profile=control.profdata ./control > control-summary.txt
cat control-summary.txt
