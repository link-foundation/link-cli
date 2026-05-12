# How link-cli Works

`clink` treats every operation as a substitution over links. A link is a triple:

```text
(index: source target)
```

A query has two sides:

```text
(restriction pattern) (substitution pattern)
```

The restriction side describes what to match. The substitution side describes
what the matched data should become.

## CRUD by Substitution

Create:

```text
() ((1 1))
```

The restriction side is empty, so the substitution side creates a link.

Read:

```text
(((1: 1 1)) ((1: 1 1)))
```

Restriction and substitution are the same, so the database is not modified. With
`--changes`, the matched link is still printed as a no-op change.

Update:

```text
((1: 1 1)) ((1: 1 2))
```

The same index appears on both sides, so the existing link is updated.

Delete:

```text
((1 2)) ()
```

The substitution side is empty, so matched links are deleted.

## Pattern Elements

Patterns can contain:

- Numeric references, such as `1` or `42`.
- Named references, such as `father` or `child`.
- Variables, such as `$index`, `$source`, and `$target`.
- Wildcards, written as `*`.
- Nested links, such as `((m a) (m a))`.
- Explicit indexes, such as `(child: father mother)`.

Variables bind during restriction matching and are reused during substitution.
Wildcards match without naming the matched value.

## Reference Validation

The query processor validates references before it writes.

Without `--auto-create-missing-references`, references must already exist or be
created in the same operation. This prevents links from pointing to missing
targets.

With `--auto-create-missing-references`, missing numeric and named references
are created as self-referential point links. For named references:

```text
(father: father father)
(mother: mother mother)
```

Those point links then become valid source and target references for other
links.

## Named References

Names are aliases for numeric link indexes. The primary database stores numeric
links; the names sidecar stores name mappings.

For `family.links`, the sidecar is:

```text
family.names.links
```

When output is formatted, numeric references with names are rendered as names.
When a named link is deleted, its name mapping is removed.

## Deduplication

The storage layer enforces uniqueness by `(source, target)`. If a query creates
the same sub-link more than once, the existing link is reused.

```text
() (((m a) (m a)))
```

This creates one `(m a)` link and then creates the outer link with that same
link as both source and target.

## LiNo Import

`--in`, `--import`, and `--lino-input` read a `.lino` file before processing the
query.

Each non-empty line is imported as one complete two-value link definition:

```text
(father: father father)
(mother: mother mother)
(child: father mother)
```

Named references are created as needed during import. Numeric indexes are
ensured before they are updated.

## LiNo Export

`--out`, `--export`, and `--lino-output` write the complete database after the
query finishes. Output is ordered by link index and names are used when
available:

```text
(father: father father)
(mother: mother mother)
(child: father mother)
```

Unnamed links are exported with numeric references:

```text
(1: 1 1)
(2: 1 2)
```

## Structure Formatting

`--structure <id>` formats one link by recursively expanding the left branch.
The formatter preserves indexes and uses a visited set to avoid infinite
recursion.

```text
(4: (3: (2: (1: 1 1) 2) 1) 2)
```

For named links, any known name is rendered instead of the numeric reference.

## Persistent Transformations

C# persistent transformations store queries as links and apply them after later
write operations.

- `--always` stores a trigger that remains active.
- `--once` stores a trigger that removes itself after it successfully applies.
- `--never` removes triggers matching the query.
- `--triggers` enables trigger evaluation explicitly.
- `--triggers-file` selects a custom trigger database.
- `--embed-triggers` stores trigger links in the main database.

The trigger schema is link-backed, using named points such as `Always`, `Once`,
`Condition`, and `Substitution`.

## Browser Runtime

The WebAssembly workbench uses the Rust query processor in the browser.

1. The `rust/wasm` `clink-wasm` crate compiles with `wasm-pack`.
2. `Clink#execute(query, optionsJson)` parses JSON options.
3. Browser storage keeps links and names in memory for the page session.
4. The Rust query processor applies the LiNo query.
5. The result returns formatted output and a structured `links` array.
6. React renders the output and graph.
7. The app mirrors the snapshot into `doublets-web` `UnitedLinks`.

Supported browser options are:

- `before`
- `changes`
- `after`
- `trace`
- `autoCreateMissingReferences`
- `structure`

The browser session is intentionally in-memory. Closing or resetting the page
clears the local database.

## Errors and Tracing

`--trace` enables verbose diagnostic output in the CLI implementations. It is
useful when debugging parser decisions, validation failures, matched solutions,
and write operations.

Common failures include:

- Malformed LiNo syntax.
- Missing references without `--auto-create-missing-references`.
- `--structure` requested for a link that does not exist.
- Import lines that are not two-value link definitions.
- Multiple trigger commands such as `--always` and `--once` in one C# command.
