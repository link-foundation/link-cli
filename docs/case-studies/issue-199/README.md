# Atomic CLI imports and queries

[Issue #199](https://github.com/link-foundation/link-cli/issues/199) reports
that a 100-line LiNo import took 19.3 seconds with `--transactions`. Each
link mutation opened an implicit transaction and checkpointed the entire
store. Killing the process left 30 of the 100 input rows in the database.
The input validation from [issue #129](https://github.com/link-foundation/link-cli/issues/129)
prevented writes for malformed LiNo. It could not roll back writes already
committed before a process stopped.

## Reproduction

Run this with a fresh database for each version of `clink`.

```bash
work=$(mktemp -d)
for i in {1..10}; do printf '(k%s: a%s b%s)\n' "$i" "$i" "$i"; done > "$work/in.lino"
clink --db "$work/data.links" --transactions --in "$work/in.lino" --trace > "$work/trace" 2>&1
grep -c 'Committed tx' "$work/trace"
```

The issue records 100 commits and 206 store saves for this input with
Rust 3.2.1. The JavaScript regression also recorded 100 commits on the
original branch. The updated Rust and JavaScript tests expect one commit
and four save traces, one for each store and name file at open and again
at commit. A 100-line Rust run with `--retention sized:64` produced the
same four save traces and one commit, and all 100 rows were readable on
reopen. The C# tests count one commit in both sync and async mode.
Running the import again with identical input leaves the journal unchanged.

## Transaction boundary

The local CLI opens one transaction before its import and query pipeline.
The transition records still describe individual link and name changes.
The database and name stores checkpoint once, with the decision synced
before the checkpoint and the applied markers synced afterward. C# writes
a complete undo image before the first mapped write so a process failure
even between a mutation and its journal append can be recovered.

The automated tests exercise a changed import followed by a mutating query
in the same invocation and a malformed query after an import under
version control. The process tests stop the CLI after it prints the
pending import and while it waits on an export FIFO. Reopening checks that
the original named row is restored and the newly imported rows and names
are absent. Existing recovery tests cover a journal decision whose
checkpoint was interrupted.
