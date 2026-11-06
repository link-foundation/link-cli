Additional change-detection false negatives found while completing link-foundation/link-cli#165 / PR https://github.com/link-foundation/link-cli/pull/166:

At snapshot `bd98923e89f90a92d49ed31bd118062321d05353`, `scripts/detect-code-changes.rs::exec_in` converts Git failures into an empty string. If Git evidence cannot be read, the fallback `ls-tree` can fail too and the helper can still emit false change flags. This is source evidence; our before/after executable probe reproduces the same failure in the older link-cli copy. The template's existing lockfile, main-merge and `--no-renames` fixes were reused in link-cli.

Reproduction: run the helper by its absolute path from a temporary directory without a Git checkout, or prepend a mock `git` executable that exits 42 for every command. Capture the exit status and `*-changed` outputs. Missing Git evidence should produce a nonzero exit and no successful no-change claim. A successful empty diff is a separate case.

Workaround: require `git rev-parse --verify HEAD` before the helper and fail the workflow on missing Git evidence. This only covers missing checkout evidence; a later diff failure still needs propagation.

Suggested code fix: return `Result<String, Error>` from Git execution and propagate failed `diff`/`ls-tree` calls with `?`; test parent existence independently before choosing the initial-commit fallback. Preserve `Ok("")` as an actual empty diff. Add tests for a missing/corrupt Git checkout and for a successful empty commit. Keep output-file write failures fatal, as the current template already does.
