#!/usr/bin/env bash
# Starts READERS (default 8) `clink --after` runs at once on a store that does
# not exist yet, TRIALS (default 3) times, and counts the failed ones. The C#
# clink crashed with AccessViolationException when readers shared the lock on
# a new store; a first run now locks it alone.
#   CLINK=csharp/Foundation.Data.Doublets.Cli/bin/Debug/net10.0/clink experiments/issue-194/fresh-readers.sh
set -u
CLINK=${CLINK:-clink}
READERS=${READERS:-8}
TRIALS=${TRIALS:-3}
status=0
for trial in $(seq "$TRIALS"); do
  dir=$(mktemp -d)
  for i in $(seq "$READERS"); do
    ( $CLINK --db "$dir/s.links" "$@" --after >/dev/null 2>"$dir/err$i" || touch "$dir/failed$i" ) 2>/dev/null &
  done
  wait
  failed=$(find "$dir" -name 'failed*' | wc -l)
  echo "trial=$trial readers=$READERS failed=$failed"
  [ "$failed" -eq 0 ] || { status=1; cat "$dir"/err* | grep -v '^\s*at ' | sort | uniq -c; }
  rm -rf "$dir"
done
exit $status
