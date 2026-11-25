#!/usr/bin/env bash
# Rust and JS clink writers on one store at once (issue #194): both take the
# same <db>.lock, so no write that exits 0 is lost.
# Usage: RUST=rust/target/debug/clink JS=js/server/bin/clink.js WRITERS=12 experiments/issue-194/mixed-writers.sh [extra args]
RUST=${RUST:-rust/target/debug/clink}; JS=${JS:-js/server/bin/clink.js}; WRITERS=${WRITERS:-12}; DIR=$(mktemp -d)
for i in $(seq 1 "$WRITERS"); do
  if [ $((i % 2)) -eq 0 ]; then CLINK="$RUST"; else CLINK="node $JS"; fi
  ( echo "(w$i: w$i w$i)" > "$DIR/in$i.lino"
    if $CLINK --db "$DIR/s.links" --in "$DIR/in$i.lino" "$@" >/dev/null 2>"$DIR/err$i.txt"; then echo ok > "$DIR/status$i"; else echo failed > "$DIR/status$i"; fi ) &
done; wait
"$RUST" --db "$DIR/s.links" --out "$DIR/out.lino" "$@" >/dev/null 2>&1
lost=0; for i in $(seq 1 "$WRITERS"); do grep -q ok "$DIR/status$i" && ! grep -q "^(w$i:" "$DIR/out.lino" && lost=$((lost+1)); done
echo "writers=$WRITERS exited_ok=$(grep -l ok "$DIR"/status* | wc -l) links_present=$(grep -c '^(w' "$DIR/out.lino") lost_after_success=$lost"
grep -h . "$DIR"/err*.txt | sort | uniq -c
rm -rf "$DIR"
[ "$lost" -eq 0 ]
