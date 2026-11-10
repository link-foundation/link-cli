#!/usr/bin/env bash
# Run with a native command and its optional prefix arguments, for example:
# bash examples/version-control/tag-names.sh rust/target/debug/clink
# bash examples/version-control/tag-names.sh dotnet path/to/clink.dll
set -euo pipefail
if (( $# == 0 )); then
  printf 'Usage: %s COMMAND [PREFIX_ARGUMENTS...]\n' "$0" >&2
  exit 2
fi
command=("$@")
directory=$(mktemp -d)
trap 'rm -rf "$directory"' EXIT
run() { "${command[@]}" --db "$directory/data.links" "$@"; }

run --vc '() ((1 1))'
run --tag origin
run --tag 'release=雪=part=origin'
run --tag 'alias=release=雪=part'
run --tag 'release=0=origin'
# Existing full tag release=0 takes priority over the numeric suffix 0.
run --tag 'numeric-alias=release=0'
run --vc '() ((2 2))'
for point in 'release=雪=part' alias 'release=0' numeric-alias; do
  actual=$(run --checkout "$point" --after)
  [[ "$actual" == '(1: 1 1)' ]]
done
run --vc '() ((2 2))'
actual=$(run --after)
[[ "$actual" == $'(1: 1 1)\n(2: 2 2)' ]]
run --list-tags
printf 'Exact tag-name checkout and following edits passed.\n'
