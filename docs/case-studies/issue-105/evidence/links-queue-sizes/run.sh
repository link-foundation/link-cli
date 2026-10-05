#!/usr/bin/env bash
# Compares the size of one store in LiNo, in links-queue's Binary Links
# Notation and in link-cli's store archive (--export-binary).
#
#   ./run.sh <links-queue checkout with npm install done> <clink binary>
set -euo pipefail
checkout=$(realpath "$1")
clink=$(realpath "$2")
here=$(dirname "$(realpath "$0")")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"

echo "| Store | LiNo | links-queue | store archive |"
echo "|---|---|---|---|"
for store in "points 10" "points 1000" "doublets 1000"; do
  set -- $store
  queue=$(node "$here/encode.mjs" "$checkout" "$1" "$2")
  rm -f db.links db.names.links db.bin
  "$clink" --db db.links --in "$1-$2.lino" > /dev/null
  "$clink" --db db.links --export-binary db.bin > /dev/null
  echo "| $2 $1 | $(wc -c < "$1-$2.lino") | $queue | $(wc -c < db.bin) |"
done
echo
node "$here/self-reference.mjs" "$checkout"
