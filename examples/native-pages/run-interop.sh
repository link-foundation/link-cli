#!/usr/bin/env bash
# CLINK_WASM is an existing web package; build examples before this script.
set -euo pipefail
cd "$(dirname "$0")/../.."
: "${CLINK_WASM:?provide the generated actual WASM web package}"
case "${NATIVE_PAGES_CHECKPOINT:-0}" in
 0|1) ;;
 *) printf '%s\n' 'NATIVE_PAGES_CHECKPOINT must be 0 or 1.' >&2; exit 2 ;;
esac
case "${NATIVE_PAGES_COMPACT:-0}" in
 0|1) ;;
 *) printf '%s\n' 'NATIVE_PAGES_COMPACT must be 0 or 1.' >&2; exit 2 ;;
esac
if [[ "${NATIVE_PAGES_COMPACT:-0}" == 1 && "${NATIVE_PAGES_CHECKPOINT:-0}" == 1 ]]; then
 printf '%s\n' 'Initial checkpoint and compacting handoffs are independent controls.' >&2; exit 2
fi
native_example="${NATIVE_PAGES_RUST_EXAMPLE:-$PWD/rust/target/debug/examples/engine_native_pages}"
managed_example="${NATIVE_PAGES_CSHARP_EXAMPLE:-$PWD/examples/native-pages-csharp/bin/Debug/net10.0/NativePagesExample.dll}"
if [[ ! -x "$native_example" || ! -f "$managed_example" ]]; then
 printf '%s\n' 'Build both native page examples first; see docs/ENGINE-NATIVE-PAGES.md.' >&2
 exit 1
fi
page_data=$(mktemp -d)
trap 'rm -rf "$page_data"' EXIT
configuration=0
check_mirrors() {
 local suffix bank
 local banks=()
 if [[ "$mode" == mirrored ]]; then
  if [[ "${NATIVE_PAGES_COMPACT:-0}" == 1 ]]; then banks=(.bank0 .bank1); else banks=(''); fi
 fi
 if [[ "$root_mode" == mirrored ]]; then banks+=(''); fi
 for bank in "${banks[@]}"; do
  for suffix in '' .meta0 .meta1; do
   cmp "$database$bank$suffix" "$database$bank.mirror$suffix"
  done
  test ! -e "$database$bank.writer"
  test ! -e "$database$bank.mirror.writer"
 done
 test ! -e "$database.writer"
 if [[ "${NATIVE_PAGES_COMPACT:-0}" == 1 ]]; then
  test ! -e "$database.bank0.writer"
  test ! -e "$database.bank1.writer"
 fi
}
damage_one_copy() {
 # Only closed, dedicated temporary journals are damaged. Each next provider
 # must repair the exact acknowledged prefix, including its metadata decision.
 local selected="$database"
 if [[ "${NATIVE_PAGES_COMPACT:-0}" == 1 ]]; then selected+=".bank$((writer % 2))"; fi
 python3 - "$selected" "$(((configuration + writer) % 2))" <<'PY'
import os, sys
path = sys.argv[1] + ('.mirror' if sys.argv[2] == '1' else '')
with open(path, 'r+b') as data:
    data.seek(56)
    value = data.read(1)
    assert len(value) == 1
    data.seek(56)
    data.write(bytes([value[0] ^ 1]))
    data.flush()
    os.fsync(data.fileno())
with open(path + '.meta0', 'r+b') as metadata:
    metadata.truncate(17)
    metadata.flush()
    os.fsync(metadata.fileno())
PY
}
damage_root_copy() {
 # Previous writers have closed. The next provider must repair the current
 # acknowledged root slot from its independently selected second copy.
 python3 - "$database" "$(((configuration + writer) % 2))" "$((writer % 2))" <<'PYROOT'
import os, sys
path = sys.argv[1] + ('.mirror' if sys.argv[2] == '1' else '') + '.meta' + sys.argv[3]
with open(path, 'r+b') as metadata:
    metadata.truncate(17)
    metadata.flush()
    os.fsync(metadata.fileno())
PYROOT
}
run_writer() {
 if [[ "$writer" -gt 0 ]]; then
  if [[ "$mode" == mirrored ]]; then damage_one_copy; fi
  if [[ "$root_mode" == mirrored ]]; then damage_root_copy; fi
 fi
 "$@"
 check_mirrors
 writer=$((writer + 1))
}
root_modes=(single)
if [[ "${NATIVE_PAGES_COMPACT:-0}" == 1 ]]; then root_modes+=(mirrored); fi
for root_mode in "${root_modes[@]}"; do
for mode in single mirrored; do
for validation in scan indexed; do
 for structure in flat shared sorted-runs; do
  for encoding in snapshot changes; do
   for physical in plain delta; do
    flags=(--references)
    [[ "${NATIVE_PAGES_COMPACT:-0}" != 1 ]] || flags+=(--compact)
    [[ "$mode" != mirrored ]] || flags+=(--mirrored)
    [[ "$root_mode" != mirrored ]] || flags+=(--mirrored-root)
    [[ "$validation" != indexed ]] || flags+=(--indexed-constraints)
    [[ "$structure" != flat ]] || flags+=(--flat)
    [[ "$structure" != sorted-runs ]] || flags+=(--sorted-runs)
    [[ "$encoding" != snapshot ]] || flags+=(--snapshot)
    [[ "$physical" != delta ]] || flags+=(--delta)
    if [[ "${NATIVE_PAGES_CHECKPOINT:-0}" == 1 ]]; then
     flags+=("--checkpoint-fixture=$PWD/docs/protocol/engine-checkpoint-vectors.json")
    fi
    database="$page_data/$root_mode-$mode-$validation-$structure-$encoding-$physical"
    if [[ "${NATIVE_PAGES_COMPACT:-0}" == 1 ]]; then printf '%s/' "$root_mode-root"; fi
    printf '%s\n' "$mode/$validation/$structure/$encoding/$physical"
    writer=0
    run_writer "$native_example" "$database" "${flags[@]}"
    run_writer node js/client/examples/engine-native-pages.mjs "$database" "${flags[@]}"
    run_writer bun js/client/examples/engine-native-pages.mjs "$database" "${flags[@]}"
    run_writer dotnet "$managed_example" "$database" "${flags[@]}"
    run_writer deno run --allow-all js/client/examples/engine-native-pages.mjs "$database" "${flags[@]}"
    run_writer "$native_example" "$database" "${flags[@]}"
    configuration=$((configuration + 1))
   done
  done
 done
done
done
done
