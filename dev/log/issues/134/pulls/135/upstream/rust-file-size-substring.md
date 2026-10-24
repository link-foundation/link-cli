## Summary

`scripts/check-file-size.rs` excludes a file when `target`, `.git` or `node_modules` occurs **anywhere in its absolute path as a substring**. Oversized sources whose names merely contain one of these words (`src/retarget.rs`, `src/target_matcher.rs`, and every Rust file under `.github/`, because `.git` is a substring of `.github`) are never measured, and in a checkout whose directory name contains one (for example a repository called `target-cli`, checked out by `actions/checkout` as `/home/runner/work/target-cli/target-cli`) **every** file is skipped and the gate reports "All files are within the line limit".

Code (template `main` at 265219d):

https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/265219d098ec1c6acbe6749562c2f3dc057f0df8/scripts/check-file-size.rs#L51-L60

```rust
let path_str = path.to_string_lossy();
EXCLUDE_PATTERNS
    .iter()
    .any(|pattern| path_str.contains(pattern))
```

`WalkDir::new(cwd)` ([L123](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/265219d098ec1c6acbe6749562c2f3dc057f0df8/scripts/check-file-size.rs#L123)) yields absolute paths, so the checkout location takes part in the match. It is also not pruned with `filter_entry`, so a real `target/` tree is still walked in full and only filtered afterwards.

## Reproduction

```bash
work=$(mktemp -d)
long() { mkdir -p "$(dirname "$1")"; yes x | head -n 1001 > "$1"; }
long "$work/rust-app/src/retarget.rs"
long "$work/rust-app/src/target_matcher.rs"
long "$work/target-cli/src/main.rs"
(cd "$work/rust-app"   && rust-script /path/to/template/scripts/check-file-size.rs); echo "exit $?"
(cd "$work/target-cli" && rust-script /path/to/template/scripts/check-file-size.rs); echo "exit $?"
```

Output (all three files have 1001 lines, the limit is 1000):

```
Checking source files for maximum 1000 lines (warning above 900) and documentation for maximum 2500 lines (warning above 2250)...

All files are within the line limit

exit 0
...
All files are within the line limit

exit 0
```

We hit this in link-foundation/link-cli (issue 134, pull request 135): `rust/src/sequences/target_matcher.rs` was never checked. The full script and its log are kept in that pull request under `dev/log/issues/134/pulls/135/scripts/template-file-size-substring-exclusion.sh` and `dev/log/issues/134/pulls/135/analysis/template-file-size-repro.txt`.

## Workaround

Do not name source files or the checkout directory with `target`, `.git` or `node_modules` in them.

## Suggested fix

Match whole path components **relative to the scanned root** and prune excluded directories while walking:

```rust
fn should_exclude(path: &Path, root: &Path) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative.components().any(|component| {
        EXCLUDE_DIRS.iter().any(|dir| component.as_os_str() == *dir)
    })
}

for entry in WalkDir::new(&cwd)
    .into_iter()
    .filter_entry(|entry| !should_exclude(entry.path(), &cwd))
{ /* ... */ }
```

plus a regression test that creates `src/retarget.rs` and `.github/scripts/tool.rs` in a temporary root and expects both to be reported. link-cli applies exactly this in `rust/scripts/check-file-size.rs` (test `exclusions_match_whole_components_not_substrings`).
