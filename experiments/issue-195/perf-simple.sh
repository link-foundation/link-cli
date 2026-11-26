#!/usr/bin/env bash
# Variant of perf.sh with LiNo every implementation accepts (C# rejects `~obj`).
# usage: perf-simple.sh <dir> <writes> [clink args...]
D=$1; N=$2; shift 2; mkdir -p "$D"; C=${CLINK:-clink}
for i in $(seq 1 "$N"); do
  printf "(s%d: s%d v%d)\n(state: state s%d)\n" $i $i $((i%3)) $i > "$D/in.lino"
  s=$(date +%s%N); $C --db "$D/s.links" --in "$D/in.lino" "$@" >/dev/null || break
  e=$(( ($(date +%s%N)-s)/1000000 ))
  if [ $((i % 10)) -eq 0 ] || [ $i -le 3 ]; then echo "write $i: ${e}ms sizes: $(stat -c '%s' $D/*.links $D/*.log 2>/dev/null | tr '\n' ' ')"; fi
done
