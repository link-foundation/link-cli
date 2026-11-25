#!/usr/bin/env bash
# Concurrent clink writers on one store (issue #194).
#
# Starts WRITERS clink processes at once, each importing one link into the
# same database, then exports the store and checks that every writer that
# exited 0 finds its link there. Extra arguments go to every clink run, so
# the same script exercises each lock kind:
#
#   CLINK="node js/server/bin/clink.js" examples/concurrent-writers/run.sh
#   CLINK=rust/target/release/clink examples/concurrent-writers/run.sh --lock pid
#   CLINK=clink examples/concurrent-writers/run.sh --no-lock   # may lose writes
#
# Exits 1 when a successful write is missing from the store.
set -u
CLINK=${CLINK:-clink}
WRITERS=${WRITERS:-12}
dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT

for i in $(seq 1 "$WRITERS"); do
  echo "(w$i: w$i w$i)" > "$dir/in$i.lino"
done
for i in $(seq 1 "$WRITERS"); do
  (
    if $CLINK --db "$dir/s.links" --in "$dir/in$i.lino" "$@" > /dev/null 2> "$dir/err$i.txt"; then
      echo ok > "$dir/status$i"
    else
      echo failed > "$dir/status$i"
    fi
  ) &
done
wait

$CLINK --db "$dir/s.links" --out "$dir/out.lino" "$@" > /dev/null 2> "$dir/export-err.txt" || true
touch "$dir/out.lino"
succeeded=0
lost=0
for i in $(seq 1 "$WRITERS"); do
  if grep -q ok "$dir/status$i"; then
    succeeded=$((succeeded + 1))
    grep -q "^(w$i:" "$dir/out.lino" || lost=$((lost + 1))
  fi
done
echo "writers=$WRITERS succeeded=$succeeded links=$(grep -c '^(w' "$dir/out.lino") lost_after_success=$lost"
cat "$dir"/err*.txt "$dir/export-err.txt" | sort | uniq -c
[ "$lost" -eq 0 ] && [ "$succeeded" -eq "$WRITERS" ]
