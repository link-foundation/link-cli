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

# Arguments prepended to every invocation of a scenario, and trigger commands
# run before its queries. Both are set by `trigger_scenario` (or by the caller,
# just before it) and cleared again by `scenario`, so the plain scenarios below
# keep running exactly as they did.
EXTRA=()
TRIGGER_SETUP=()

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

  # Trigger commands come in (flag, query) pairs and run before the queries.
  # Their stdout is compared too: it carries the address the trigger was stored
  # at, and how many triggers a --never removed.
  local i
  for ((i = 0; i < ${#TRIGGER_SETUP[@]}; i += 2)); do
    run_command "$RS" "$rs_dir" "${TRIGGER_SETUP[i]}" "${TRIGGER_SETUP[i + 1]}"
    run_command "$CS" "$cs_dir" "${TRIGGER_SETUP[i]}" "${TRIGGER_SETUP[i + 1]}"
  done

  local q
  for q in "$@"; do
    "$RS" --db "$rs_dir/l.links" ${EXTRA[@]+"${EXTRA[@]}"} --query "$q" > "$rs_dir/out" 2>&1
    verdict "$?" "$q" >> "$rs_dir/status"
    "$CS" --db "$cs_dir/l.links" ${EXTRA[@]+"${EXTRA[@]}"} --query "$q" > "$cs_dir/out" 2>&1
    verdict "$?" "$q" >> "$cs_dir/status"
  done

  dump "$RS" "$rs_dir"
  dump "$CS" "$cs_dir"
}

# Runs one non-query command and records both its verdict and its stdout.
# stderr is dropped: the two implementations agree on *what* they reject, not on
# how they word it.
run_command() {
  local bin="$1" dir="$2"; shift 2
  "$bin" --db "$dir/l.links" ${EXTRA[@]+"${EXTRA[@]}"} "$@" > "$dir/out" 2> /dev/null
  verdict "$?" "$*" >> "$dir/status"
  sed 's/^/  /' "$dir/out" >> "$dir/status"
}

# The final database, plus the trigger sidecar when the scenario created one, so
# that how a trigger is *stored* is compared and not just what it did.
dump() {
  local bin="$1" dir="$2"
  "$bin" --db "$dir/l.links" --after > "$dir/final" 2>&1
  if [ -f "$dir/l.triggers.links" ]; then
    echo "triggers:" >> "$dir/final"
    "$bin" --db "$dir/l.triggers.links" --after >> "$dir/final" 2>&1
  fi
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
  EXTRA=(); TRIGGER_SETUP=()

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
  EXTRA=(); TRIGGER_SETUP=()

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

# A substitution half that no restriction bound -- a never-bound variable, or a
# `*` -- is *unspecified*, not an address. Creating from one writes null there,
# looking one up treats it as a wildcard, and updating through one keeps the
# half already stored.
scenario "unbound variable point"      '() (($a $a))'
scenario "unbound variable twice"      '() (($a $a))' '() (($a $a))'
scenario "unbound variable at an index" '() ((5: $a $a))'
scenario "unbound variable one half"   '() ((1 1))' '() ((1 $a))'
scenario "star in a substitution"      '() ((* *))'
scenario "unbound variable in an update" '() ((1 1))' '((1: 1 1)) ((1: $x $y))'

# Which address a new link gets is observable, so the two stores have to hand
# out addresses in the same order: a freed address is reused before the store
# grows, the most recently freed one first, and freeing the last link shrinks
# the store instead of leaving a hole.
scenario "reuse a freed address"      '() ((1 1) (2 2) (3 3))' '((2: 2 2)) ()' '() ((1 3))'
scenario "reuse after a shrink"       '() ((1 1) (2 2) (3 3))' '((3: 3 3)) ()' '() ((1 2))'
scenario "reuse the newest hole first" '() ((1 1) (2 2) (3 3) (4 4) (5 5))' \
  '((2: 2 2)) ()' '((4: 4 4)) ()' '() ((1 3))' '() ((3 1))' '() ((1 5))'

# Reaching a requested address creates the addresses before it too, and those
# have to be given back -- they were never asked for.
EXTRA=(--auto-create-missing-references)
scenario "auto-create frees the addresses it passed over" \
  '() ((1 1) (2 2) (3 3))' '((2: 2 2)) ()' '((3: 3 3)) ()' '() ((1 4))'
EXTRA=(--auto-create-missing-references)
scenario "auto-create leaves the new link the first address" '(() ((1 2)))'

# trigger_scenario <name> [<trigger-flag> <trigger-query>]... -- <query>...
#
# Stores (or removes) persistent transformation triggers, then runs the queries.
# Every invocation gets --auto-create-missing-references so that a substitution
# introducing a reference the database does not have yet can actually be
# applied; without it both CLIs would merely agree on refusing to fire.
trigger_scenario() {
  local name="$1"; shift
  TRIGGER_SETUP=()
  while [ "$#" -gt 0 ] && [ "$1" != "--" ]; do
    TRIGGER_SETUP+=("$1"); shift
  done
  shift
  EXTRA+=(--auto-create-missing-references)
  scenario "$name" "$@"
}

trigger_scenario "always trigger fires" \
  --always '(((1: 1 1)) ((1: 1 2)))' -- '() ((1: 1 1))'
trigger_scenario "always trigger keeps firing" \
  --always '(((1: 1 1)) ((1: 1 2)))' -- '() ((1: 1 1))' '((1: 1 2)) ((1: 1 1))'
trigger_scenario "once trigger fires only once" \
  --once '(((1: 1 1)) ((1: 1 2)))' -- '() ((1: 1 1))' '((1: 1 2)) ((1: 1 1))'
trigger_scenario "never removes a stored trigger" \
  --always '(((1: 1 1)) ((1: 1 2)))' --never '(((1: 1 1)) ((1: 1 2)))' -- '() ((1: 1 1))'
trigger_scenario "never on an empty trigger store" \
  --never '(((1: 1 1)) ((1: 1 2)))' -- '() ((1: 1 1))'
trigger_scenario "trigger without a match stays dormant" \
  --always '(((7: 7 7)) ((7: 7 8)))' -- '() ((1: 1 1))'

EXTRA=(--embed-triggers)
trigger_scenario "trigger embedded in the main database" \
  --always '(((1: 1 1)) ((1: 1 2)))' -- '() ((1: 1 1))'

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
