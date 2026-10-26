#!/usr/bin/env bash
# CLINK_WASM is an existing web package; build examples before this script.
set -euo pipefail
cd "$(dirname "$0")/../.."
: "${CLINK_WASM:?provide the generated actual WASM web package}"
case "${NATIVE_PAGES_CHECKPOINT:-0}" in
 0|1) ;;
 *) printf '%s\n' 'NATIVE_PAGES_CHECKPOINT must be 0 or 1.' >&2; exit 2 ;;
esac
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
 local suffix
 for suffix in '' .meta0 .meta1; do
  cmp "$database$suffix" "$database.mirror$suffix"
 done
 test ! -e "$database.writer"
 test ! -e "$database.mirror.writer"
}
damage_one_copy() {
 # Only closed, dedicated temporary journals are damaged. Each next provider
 # must repair the exact acknowledged prefix, including its metadata decision.
 python3 - "$database" "$(((configuration + writer) % 2))" <<'PY'
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
run_writer() {
 if [[ "$mode" == mirrored && "$writer" -gt 0 ]]; then damage_one_copy; fi
 "$@"
 if [[ "$mode" == mirrored ]]; then check_mirrors; fi
 writer=$((writer + 1))
}
for mode in single mirrored; do
for validation in scan indexed; do
 for structure in flat shared sorted-runs; do
  for encoding in snapshot changes; do
   for physical in plain delta; do
    flags=(--references)
    [[ "$mode" != mirrored ]] || flags+=(--mirrored)
    [[ "$validation" != indexed ]] || flags+=(--indexed-constraints)
    [[ "$structure" != flat ]] || flags+=(--flat)
    [[ "$structure" != sorted-runs ]] || flags+=(--sorted-runs)
    [[ "$encoding" != snapshot ]] || flags+=(--snapshot)
    [[ "$physical" != delta ]] || flags+=(--delta)
    if [[ "${NATIVE_PAGES_CHECKPOINT:-0}" == 1 ]]; then
     flags+=("--checkpoint-fixture=$PWD/docs/protocol/engine-checkpoint-vectors.json")
    fi
    database="$page_data/$mode-$validation-$structure-$encoding-$physical"
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
