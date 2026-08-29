#!/usr/bin/env bash
# Runs the same sequence of `clink` queries through the C# and the Rust CLI and
# diffs the resulting databases, so cross-language behaviour gaps are evidence,
# not guesswork.
#
# Usage: docs/case-studies/issue-100/evidence/cli-parity/run.sh [rust-binary] [csharp-binary]
set -u

REPO="$(cd "$(dirname "$0")/../../../../.." && pwd)"
RS="${1:-$REPO/rust/target/debug/clink}"
CS="${2:-$REPO/csharp/Foundation.Data.Doublets.Cli/bin/Debug/net10.0/clink}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

failures=0

# Runs the query sequence through both CLIs and leaves, for each of them, the
# final database dump in "$WORK/<lang>/final" and one accepted/rejected verdict
# per query in "$WORK/<lang>/status".
#
# The verdicts are compared alongside the dumps because a query the two CLIs
# both refuse leaves two empty databases, which a dump-only comparison would
# happily call a match. The exit status is compared rather than the message
# text: the two implementations are expected to agree on *what* they accept,
# not on how they word a rejection.
run_both() {
  local rs_dir="$WORK/rs" cs_dir="$WORK/cs"
  rm -rf "$rs_dir" "$cs_dir"; mkdir -p "$rs_dir" "$cs_dir"
  : > "$rs_dir/status"; : > "$cs_dir/status"

  local q
  for q in "$@"; do
    "$RS" --db "$rs_dir/l.links" --query "$q" > "$rs_dir/out" 2>&1
    verdict "$?" "$q" >> "$rs_dir/status"
    "$CS" --db "$cs_dir/l.links" --query "$q" > "$cs_dir/out" 2>&1
    verdict "$?" "$q" >> "$cs_dir/status"
  done

  "$RS" --db "$rs_dir/l.links" --after > "$rs_dir/final" 2>&1
  "$CS" --db "$cs_dir/l.links" --after > "$cs_dir/final" 2>&1
}

verdict() {
  if [ "$1" -eq 0 ]; then echo "accepted: $2"; else echo "rejected: $2"; fi
}

# Succeeds when both CLIs accepted the same queries and ended at the same
# database.
agree() {
  diff -q "$WORK/rs/status" "$WORK/cs/status" > /dev/null \
    && diff -q "$WORK/rs/final" "$WORK/cs/final" > /dev/null
}

report_divergence() {
  echo "      queries: $*"
  if ! diff -q "$WORK/rs/status" "$WORK/cs/status" > /dev/null; then
    echo "      accepted queries differ:"
    diff "$WORK/rs/status" "$WORK/cs/status" | sed 's/^/        /'
  fi
  echo "      rust:"; sed 's/^/        /' "$WORK/rs/final"
  echo "      c#:";   sed 's/^/        /' "$WORK/cs/final"
}

# scenario <name> <query>...
scenario() {
  local name="$1"; shift
  run_both "$@"

  if agree; then
    echo "PASS  $name"
  else
    failures=$((failures + 1))
    echo "FAIL  $name"
    report_divergence "$@"
  fi
}

# known_difference <name> <reason> <query>...
#
# A scenario the two CLIs are *expected* to answer differently because of a
# defect in a dependency rather than in this repository. It does not count as a
# failure, but agreement does: the day the upstream fix lands, this turns red so
# the exemption gets removed instead of quietly outliving its reason.
known_difference() {
  local name="$1" reason="$2"; shift 2
  run_both "$@"

  if agree; then
    failures=$((failures + 1))
    echo "FAIL  $name (the languages now agree -- drop the exemption)"
    echo "      reason on record: $reason"
  else
    echo "KNOWN $name"
    echo "      $reason"
    report_divergence "$@"
  fi
}

scenario "create"                     '() ((1 1))'
scenario "duplicate create"           '() ((1 1))' '() ((1 1))'
scenario "update target"              '() ((1 1))' '() ((2 2))' '((1: 1 1)) ((1: 1 2))'
scenario "delete point"               '() ((1 1))' '((1: 1 1)) ()'
scenario "cascade delete of usage"    '() ((1 1))' '() ((2 2))' '() ((1 2))' '((2: 2 2)) ()'
scenario "cascade delete chain"       '() ((1 1))' '() ((2 2))' '() ((1 2))' '() ((3 3))' '((1: 1 1)) ()'
scenario "uniqueness on update"       '() ((1 1))' '() ((2 2))' '() ((1 2))' '() ((2 1))' '((4: 2 1)) ((4: 1 2))'
scenario "delete with contents"       '() ((1 1))' '() ((2 2))' '() ((1 2))' '((3: 1 2)) ()'
scenario "named create"               '() ((name: name name))'
scenario "named cascade delete"       '() ((a: a a))' '() ((b: b b))' '() ((a b))' '((a: a a)) ()'
scenario "swap one link"              '() ((1 1) (1 2))' '((2: 1 2)) ((2: 2 1))'
scenario "swap all links"             '() ((1 2) (2 1))' '((($index: $source $target)) (($index: $target $source)))'
scenario "no-op variable query"       '() ((1 1) (2 2))' '((($index: $source $target)) (($index: $source $target)))'
scenario "delete by wildcard"         '() ((1 1) (2 2) (1 2))' '((* 1 *)) ()'
scenario "delete everything"          '() ((1 1) (2 2) (1 2))' '((*: * *)) ()'
scenario "rename named link"          '() ((child: father mother))' '(((child: father mother)) ((son: father mother)))'
scenario "nested composite create"    '() ((a (b c)))'
scenario "explicit index after gap"   '() ((5: 5 5))'
scenario "reverse update chain"       '() ((1 1))' '() ((2 2))' '((1: 1 1)) ((1: 1 2))' '((1: 1 2)) ((1: 1 1))'
scenario "point to non-point"         '() ((1 1))' '((1: 1 1)) ((1: 0 0))'
scenario "delete self referencing"    '() ((1 1))' '() ((1 1) (1 1))' '((1: 1 1)) ()'

known_difference "update into duplicate" \
  "Platform.Data.Doublets 0.18.1 MergeUsages corrupts the usages it repoints (see ../csharp-merge-usages), so C# leaves (2: 2 0) where doublets-rs rebases the usage onto the surviving link and leaves (2: 2 2)." \
  '() ((1 2) (2 1))' '((1: 1 2)) ((1: 2 1))'

echo
if [ "$failures" -eq 0 ]; then
  echo "All scenarios match, except the known upstream differences listed above."
else
  echo "$failures scenario(s) diverge."
fi
exit "$failures"
