#!/usr/bin/env bash
# Reproduces the DeepClient CI flake seen on PR #178 (run for 0da1a780):
#   rm: cannot remove '/tmp/tmp.XXXX': Directory not empty
# examples/deep-client/run.sh killed the server and removed its database
# folder without waiting, so a server still writing on its way out raced rm.
# The stand-in server below keeps writing files into the folder for a moment
# after SIGTERM, as a store flushing on shutdown would; 3000 existing files
# give rm a realistic amount of work so the late writes land mid-removal.
# Usage: cleanup-race.sh old|new [iterations]
set -uo pipefail
mode=${1:-old}
iterations=${2:-20}
failures=0
for _ in $(seq 1 "$iterations"); do
  dir=$(mktemp -d)
  (cd "$dir" && seq 1 3000 | xargs touch)
  bash -c 'trap "for i in \$(seq 1 2000); do : >\"$0/late-\$i\"; done; exit 0" TERM
           while :; do : >"$0/data"; sleep 0.01; done' "$dir" 2>/dev/null &
  server=$!
  sleep 0.1
  kill "$server" 2>/dev/null || true
  if [ "$mode" = new ]; then
    for _ in $(seq 1 50); do kill -0 "$server" 2>/dev/null || break; sleep 0.2; done
    kill -9 "$server" 2>/dev/null || true
    wait "$server" 2>/dev/null || true
  fi
  rm -rf "$dir" 2>/dev/null || failures=$((failures + 1))
  wait "$server" 2>/dev/null
  rm -rf "$dir"
done
echo "$mode: $failures of $iterations cleanups failed"
[ "$failures" -eq 0 ]
