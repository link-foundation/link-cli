# link-cli Requirements

This document summarizes repository requirements collected from GitHub issues,
pull requests, PR comments, and the current codebase. The evidence for the
issue-71 documentation refresh is stored in
`docs/case-studies/issue-71/evidence/`.

## Current Product Scope

`clink` manipulates a links database with one LiNo substitution expression:

```text
(matching pattern) (substitution pattern)
```

The same expression model covers create, read, update, delete, and mixed
operations. Each stored link is a triple:

```text
(index: source target)
```

The production NuGet tool is implemented in C#. The Rust implementation mirrors
the core CLI behavior and is also used by the WebAssembly browser workbench.

## Implemented Requirements

| Requirement | Source | Status |
|-------------|--------|--------|
| Parse a query passed through the command line. | #1, #6 | Implemented with positional query plus `--query`, `--apply`, and `--do`. |
| Create, read, update, and delete links through substitution. | README examples, #1, #2, #4, #5 | Implemented in C# and Rust query processors. |
| Report changes only when requested. | #7, #9, #16, #26, #27, #28, #42 | `--changes` prints copyable LiNo-style before/after pairs, and simplification removes intermediate noise. |
| Print database state before and after a query. | #9 | `--before`, `--after`, and `--links` are implemented. |
| Support explicit link indexes. | #5 | Queries can create or update `(id: source target)` when references are valid. |
| Validate references. | #15, #51 | Missing numeric and named references fail unless they will be created in the same operation or auto-create is enabled. |
| Auto-create missing references. | #51 | `--auto-create-missing-references` creates missing numeric and named references as self-referential point links. |
| Support variables. | #13, #14 | `$name` variables can bind index, source, and target positions. |
| Support deep and wildcard patterns. | #10 | Nested LiNo patterns and `*` wildcards are supported by the query processors. |
| Support full string IDs and names. | #11, #23, #29, #45, #53 | Names are implemented through `NamedTypesDecorator` and a sidecar names database. |
| Clean up names when links are deleted. | #11, #23 | Deleting a named link removes its name mapping. |
| Deduplicate identical links and sub-links. | #65, #66 | Creation reuses existing `(source target)` pairs instead of storing duplicates. |
| Export the database as LiNo. | #8, #24, #44, #54 | `--out`, `--export`, and `--lino-output` write a complete LiNo file. |
| Import a database from LiNo. | #25, #43 | `--in`, `--import`, and `--lino-input` read a LiNo file before query execution. |
| Format a link structure. | #19, #48 | `--structure` recursively formats the left branch with indexes preserved. |
| Store persistent transformations. | #3, #55 | C# supports `--always`, `--once`, `--never`, `--triggers`, `--triggers-file`, and `--embed-triggers`. |
| Separate code by implementation language. | #63, #64 | C# code lives under `csharp/`; Rust code lives under `rust/`; WebAssembly wrapper code lives at the repository root plus `web/`. |
| Provide Rust parity for core behavior. | #63, #67, #68 | Rust mirrors query processing, names, import/export, structure formatting, and Unicode sequence support. |
| Run in a browser. | #12, #52, #69, #70 | The Rust query processor is wrapped with `wasm-bindgen` and surfaced through a React/Vite workbench. |
| Keep CI split by surface area. | #63, #69 | Separate C#, Rust, and WebAssembly workflows run relevant checks. |

## Open Requirements

| Requirement | Source | Current state |
|-------------|--------|---------------|
| Unify shell quoting across platforms. | #18, PR #49 | Open. Documentation still uses Unix-style single quotes in many examples because that is the current repository convention. |
| LiNo protocol server mode. | #30, PR #41 | Open. Proposed server mode is not part of the merged CLI. |
| Benchmark CLI versus protocol server access. | #31, PR #40 | Open. Depends on server mode. |
| LINO REST API. | #32, PR #39 | Open. Not part of the merged CLI. |
| LINO GRPC-style API. | #33, PR #38 | Open. Not part of the merged CLI. |
| LINO GraphQL-style API. | #34, PR #37 | Open. Not part of the merged CLI. |
| Benchmark API transport protocols. | #35, PR #36 | Open. Depends on REST/GRPC/GraphQL API work. |
| MCP support for neural network memory. | #56, PR #57 | Open. Not part of the merged CLI. |
| SPARQL/RDF-compatible API. | #59 | Open. |
| SQL/PostgreSQL-compatible API. | #60 | Open. |

## Implementation Parity Requirements

When adding a code feature:

- Prefer matching behavior in C# and Rust unless the issue explicitly scopes a
  feature to one implementation.
- Add tests in the implementation-specific test tree.
- Keep Rust tests in separate files under `rust/tests/`.
- Add C# changesets or Rust changelog fragments when code changes affect a
  released package.
- Keep documentation aligned with the actual help text and supported aliases.

## Documentation Requirements

The README should remain the quick-start document and show:

- Basic numbered CRUD examples.
- Equivalent named-reference examples.
- Variables, wildcard/deep patterns, and deduplication behavior.
- Import, export, structure formatting, output flags, and storage files.
- Trigger options and their C#-only status.
- Links to deeper architecture and behavior documentation.

The deeper docs should explain:

- What data is stored in each database file.
- How a LiNo query becomes restrictions, substitutions, matches, and writes.
- How names, pinned types, imports, exports, and triggers are implemented.
- Which dependencies are used and why.
- Which requirements are implemented versus planned.
