#!/usr/bin/env bash
# Reproduces issue #194: N concurrent clink writers, each adds one link to the
# same store. Every writer that exits 0 must find its link in the final store.
# Usage: CLINK=path/to/clink WRITERS=12 experiments/issue-194/concurrent-writers.sh [extra clink args]
CLINK=${CLINK:-clink}; WRITERS=${WRITERS:-12}; DIR=$(mktemp -d)
for i in $(seq 1 "$WRITERS"); do
  ( echo "(w$i: w$i w$i)" > "$DIR/in$i.lino"
    if $CLINK --db "$DIR/s.links" --in "$DIR/in$i.lino" "$@" >/dev/null 2>"$DIR/err$i.txt"; then echo ok > "$DIR/status$i"; else echo failed > "$DIR/status$i"; fi ) &
done; wait
$CLINK --db "$DIR/s.links" --out "$DIR/out.lino" "$@" >/dev/null 2>&1
lost=0; for i in $(seq 1 "$WRITERS"); do grep -q ok "$DIR/status$i" && ! grep -q "^(w$i:" "$DIR/out.lino" && lost=$((lost+1)); done
echo "writers=$WRITERS exited_ok=$(grep -l ok "$DIR"/status* | wc -l) links_present=$(grep -c '^(w' "$DIR/out.lino") lost_after_success=$lost"
grep -h . "$DIR"/err*.txt | sort | uniq -c
ls -a "$DIR" | grep -v '^\(in\|err\|status\)' | tr '\n' ' '; echo
rm -rf "$DIR"
[ "$lost" -eq 0 ]
