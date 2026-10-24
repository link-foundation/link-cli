# Issue #129: validate LiNo imports before writing

Issue: <https://github.com/link-foundation/link-cli/issues/129>

## Reproduction and cause

```sh
printf '(a: a a)\n(b: b b)\n(c: c\n' > bad.lino
clink --db data.links --import bad.lino --auto-create-missing-references
clink --db data.links --export after.lino
```

The C# importer previously returned exit code 1 with `Invalid LiNo import line
3: Expected reference.`, but `after.lino` contained the first two definitions.
`ParseDefinitions` was an iterator consumed inside the mutation loop. Each
definition was written before the next line was parsed. Empty quoted source or
target references were also rejected only during mutation.

Rust parsed the entire input first, but checked import-specific shape constraints
during mutation. For example, importing `(a: a a)` followed by `(c: c)` returned
an error while leaving `a` in a library caller's in-memory store. The Rust CLI
already avoided saving these changes on failure; a library caller could still
observe or save them.

## Change

C# materializes all definitions and validates source/target references before
the write loop. Rust validates every definition and nested reference before its
write loop. Both use the existing successful import path afterward. File APIs
already read the whole file before importing, so read failures also precede
mutation. No transaction flag or temporary replacement database is needed.

This implements the issue's suggested parse-before-create solution. Its scope
is invalid input: it does not add rollback for storage errors during valid
imports or change crash durability. Binary archive imports and a separate query
in the same invocation keep their existing behavior.

## Requirements and evidence

| Requirement | Verification |
| --- | --- |
| Reject a malformed final line without retaining earlier definitions | C# `ImportText_InvalidLastLineLeavesLinksAndNamesUnchanged`; Rust `invalid_import_leaves_links_and_names_unchanged` |
| Keep an existing database's links and names unchanged | Both library regressions cover populated and empty stores, mutations of existing links, and creation of new names |
| Exit 1 and preserve state after reopening | C# `InvalidLastLine_LeavesDatabaseUnchangedAfterReopening`; Rust `invalid_import_preserves_database_after_reopening` compare binary exports, including addresses and names, from separate processes |
| Support all LiNo input aliases and optional decorators | CLI tests cover `--import`, `--in`, `--lino-input`, and transactions; Rust also covers indexed lookups |
| Preserve successful imports | Existing numeric, named, quoted-name, round-trip, and indexed-import tests |

Before the fix, the focused C# run had 9 failures and 6 passes, and the Rust
library regression failed with a nonempty store where an empty one was expected.
After the fix, these regressions pass. Local investigation logs are kept in the
ignored `ci-logs/` directory beside this document.

Run the regression suites with:

```sh
dotnet test csharp --configuration Release --filter 'FullyQualifiedName~LinoDatabaseInputTests|FullyQualifiedName~CliImportAtomicityTests'
cargo test --manifest-path rust/Cargo.toml --test lino_database_input_tests --test cli_import_tests
```
