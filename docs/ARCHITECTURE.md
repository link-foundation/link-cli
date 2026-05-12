# link-cli Architecture

`link-cli` is a multi-implementation repository centered on the same data
model: each link is a doublet with an index, source, and target.

```text
(index: source target)
```

The CLI accepts LiNo substitution expressions and applies them to a persistent
links database.

## Repository Layout

| Path | Responsibility |
|------|----------------|
| `README.md` | User-facing quick start and feature overview. |
| `docs/` | Requirements, architecture notes, behavior notes, screenshots, and case studies. |
| `csharp/` | Production .NET CLI package published as the `clink` tool. |
| `rust/` | Rust library and native `clink` binary that mirror core C# behavior. |
| `csharp/scripts/` | C# release, changeset, and repository validation helpers. |
| `rust/scripts/` | Rust release, changelog, crate publication, and repository validation helpers. |
| `rust/wasm/` | `wasm-bindgen` wrapper crate around the Rust query processor. |
| `web/` | React/Vite browser workbench. |
| `.github/workflows/` | Split C#, Rust, and WebAssembly CI workflows. |

## C# Implementation

The C# implementation is the production NuGet tool.

Key files:

- `csharp/Foundation.Data.Doublets.Cli/Program.cs`: command-line surface and
  command orchestration.
- `AdvancedMixedQueryProcessor.cs`: LiNo restriction/substitution processing.
- `NamedTypesDecorator.cs`: names layered over links with a sidecar names
  database.
- `PinnedTypesDecorator.cs`: pinned type support composed into named types.
- `LinoDatabaseInput.cs`: `.lino` import.
- `LinoDatabaseOutput.cs`: database export, change formatting, and structure
  formatting.
- `PersistentTransformationDecorator.cs`: stored trigger support.

Main C# dependencies:

- `Platform.Data`
- `Platform.Data.Doublets`
- `Platform.Data.Doublets.Sequences`
- `Link.Foundation.Links.Notation`
- `System.CommandLine`

## Rust Implementation

The Rust implementation mirrors the core CLI behavior and is the basis for the
browser runtime.

Key files:

- `rust/src/main.rs`: native `clink` entry point.
- `rust/src/cli.rs`: argument parser and help text.
- `rust/src/query_processor.rs`: LiNo query execution.
- `rust/src/query_processor_substitution.rs`: substitution preservation helpers.
- `rust/src/link_reference_validator.rs`: reference validation and auto-create.
- `rust/src/named_types.rs`: names sidecar storage.
- `rust/src/lino_database_input.rs`: `.lino` import.
- `rust/src/sequences/`: Unicode sequence conversion and related parity code.

Main Rust dependencies:

- `doublets = "0.3.0"` for links storage foundations.
- `links-notation = "0.13.0"` for LiNo parsing.
- `lino-arguments = "0.3.0"` for argument initialization compatibility.
- `anyhow` and `thiserror` for error handling.

The Rust CLI currently supports the core query, storage, import/export,
structure, output, named-reference, and auto-create options. Persistent
transformation trigger CLI options are implemented in C# only.

## WebAssembly Workbench

The browser workbench is a wrapper around the Rust query processor.

Key files:

- `rust/wasm/src/lib.rs`: exports `Clink` with `execute`, `snapshot`, `reset`, `version`,
  and `rustCoreVersion`.
- `web/src/App.jsx`: React application, query editor, graph view, and runtime
  status.
- `web/src/styles.css`: application styling.
- `web/vite.config.js`: Vite build configuration.

Runtime flow:

1. Vite loads the generated `clink-wasm` package.
2. `Clink` stores links in an in-memory `BrowserStorage`.
3. Queries are passed into the Rust `QueryProcessor`.
4. The result includes formatted output plus a structured `links` snapshot.
5. React renders the snapshot and mirrors it into `doublets-web` `UnitedLinks`.

The committed lockfile currently pins `doublets-web` to `0.1.2`.

## Data Files

`--db` selects the primary links database file. Companion files are derived from
the primary filename.

| File pattern | Owner | Purpose |
|--------------|-------|---------|
| `<name>.links` | C# and Rust | Primary numeric links database. |
| `<name>.names.links` | C# and Rust | Mapping between string names and numeric link references. |
| `<name>.triggers.links` | C# | Persistent trigger definitions when triggers are not embedded. |

For `graph.links`, the default names file is `graph.names.links`, and the
default triggers file is `graph.triggers.links`.

## Query Pipeline

The high-level pipeline is the same across C# and Rust:

1. Parse the LiNo input into local link-pattern structures.
2. Split the top-level expression into restriction and substitution sides.
3. Validate references, optionally creating missing point links.
4. Find all database links that satisfy the restriction patterns.
5. Resolve variables from each match into substitution patterns.
6. Determine create, read/no-op, update, and delete operations.
7. Apply writes to storage.
8. Format requested output: before, changes, after, structure, import/export.

## Persistent Transformation Triggers

C# trigger support stores transformation queries as links. `--always` and
`--once` create trigger records, `--never` removes matching records, and normal
write operations apply stored triggers afterward. One-shot triggers delete
themselves after a successful application.

Trigger storage can be:

- A default sidecar file derived from the primary database.
- A custom file selected by `--triggers-file`.
- The main database itself when `--embed-triggers` is enabled.

## CI Workflows

| Workflow | Scope |
|----------|-------|
| `.github/workflows/csharp.yml` | .NET restore, build, tests, package, and release. |
| `.github/workflows/rust.yml` | Rust formatting, clippy, file-size gate, tests, package, and release. |
| `.github/workflows/wasm.yml` | Rust core tests, wasm-pack tests, Vite build, artifact upload, and manual Pages deployment. |

Path filters keep most workflows focused on the parts of the repository they
own.

## External References

- NuGet `clink`: https://www.nuget.org/packages/clink
- crates.io `link-cli`: https://crates.io/crates/link-cli
- Rust `doublets`: https://docs.rs/doublets/latest/doublets/
- Rust `links-notation`: https://docs.rs/crate/links-notation/0.13.0
- npm `doublets-web`: https://www.npmjs.com/package/doublets-web
- WebAssembly local docs: [../README-WASM.md](../README-WASM.md)
- WebAssembly implementation notes: [../WEBASSEMBLY_IMPLEMENTATION.md](../WEBASSEMBLY_IMPLEMENTATION.md)
