# link-cli

[![C# CI/CD Pipeline](https://github.com/link-foundation/link-cli/actions/workflows/csharp.yml/badge.svg)](https://github.com/link-foundation/link-cli/actions/workflows/csharp.yml)
[![Rust CI/CD Pipeline](https://github.com/link-foundation/link-cli/actions/workflows/rust.yml/badge.svg)](https://github.com/link-foundation/link-cli/actions/workflows/rust.yml)
[![WebAssembly CI](https://github.com/link-foundation/link-cli/actions/workflows/wasm.yml/badge.svg)](https://github.com/link-foundation/link-cli/actions/workflows/wasm.yml)
[![NuGet (clink)](https://img.shields.io/nuget/v/clink?logo=nuget&label=clink)](https://www.nuget.org/packages/clink)
[![NuGet (library)](https://img.shields.io/nuget/v/Foundation.Data.Doublets.Cli?logo=nuget&label=Foundation.Data.Doublets.Cli)](https://www.nuget.org/packages/Foundation.Data.Doublets.Cli)
[![Crates.io](https://img.shields.io/crates/v/link-cli?logo=rust&label=Crates.io)](https://crates.io/crates/link-cli)
[![Docs.rs](https://docs.rs/link-cli/badge.svg)](https://docs.rs/link-cli)
[![C# Release](https://img.shields.io/github/v/release/link-foundation/link-cli?filter=csharp-v*&label=C%23%20release)](https://github.com/link-foundation/link-cli/releases?q=C%23&expanded=true)
[![Rust Release](https://img.shields.io/github/v/release/link-foundation/link-cli?filter=rust-v*&label=Rust%20release)](https://github.com/link-foundation/link-cli/releases?q=Rust&expanded=true)

`clink` (`CLInk` `cLINK`), a CLI tool to manipulate links using single substitution operation.

It is based on [associative theory](https://habr.com/ru/articles/895896) (also in [ru](https://habr.com/ru/articles/804617)) and [Links Notation](https://github.com/linksplatform/Protocols.Lino) (also in [ru](https://github.com/linksplatform/Protocols.Lino/blob/main/README.ru.md))

It includes a production C# CLI/library pair on NuGet, a Rust CLI/library
crate on Crates.io, and a Rust-powered WebAssembly browser workbench built on
[links data store](https://github.com/linksplatform?view_as=public)
concepts (see also in [ru](https://github.com/linksplatform/.github/blob/main/profile/README.ru.md)).
Both ecosystems ship a runnable CLI and a reusable public library with full
auto-generated API documentation (DocFX for C#, `cargo doc`/docs.rs for Rust)
so external projects can embed the parser, query processors, decorators,
named/pinned types, persistent transformation triggers, and LiNo I/O without
re-implementing any of the internals.

## WebAssembly Browser Workbench

`clink` can run in the browser through the Rust query processor compiled to
WebAssembly. The React workbench mirrors the current link set into
[`doublets-web`](https://www.npmjs.com/package/doublets-web), the WebAssembly
package built from `doublets-rs`.

- Live demo: <https://link-foundation.github.io/link-cli/>
- Browser app documentation and implementation notes: [js/README.md](js/README.md)

## Documentation

- [docs/REQUIREMENTS.md](docs/REQUIREMENTS.md): implemented and planned requirements collected from issues and PR comments.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): repository layout, major components, dependencies, storage files, and CI.
- [docs/HOW-IT-WORKS.md](docs/HOW-IT-WORKS.md): deeper explanation of query processing, references, import/export, triggers, and the WebAssembly workbench.
- [docs/case-studies/issue-71/README.md](docs/case-studies/issue-71/README.md): evidence and analysis behind the original documentation refresh.
- [docs/case-studies/issue-92/README.md](docs/case-studies/issue-92/README.md): evidence and analysis behind the dual CLI + library packaging and unified API documentation site.
- [docs/case-studies/issue-94/README.md](docs/case-studies/issue-94/README.md): evidence and analysis for the optional transactions and version-control layers.

### API references

- C# library: package page at <https://www.nuget.org/packages/Foundation.Data.Doublets.Cli> and the DocFX-generated reference on [GitHub Pages](https://link-foundation.github.io/link-cli/csharp/).
- Rust library: <https://docs.rs/link-cli> (also mirrored on [GitHub Pages](https://link-foundation.github.io/link-cli/rust/link_cli/)).
- Combined landing page: <https://link-foundation.github.io/link-cli/>.

## Installation

Language package documentation:

- [C# NuGet tool and library](csharp/README.md)
- [Rust crate (CLI + library)](rust/README.md)

Each language ships **both** a CLI binary and a public library, so external
projects can either run the tool directly or pull in the parser, query
processors, decorators, and LiNo I/O via a package reference.

### C# (.NET)

```bash
# CLI: install the `clink` command globally.
dotnet tool install --global clink

# Library: embed the parser, processors, and decorators in another .NET project.
dotnet add package Foundation.Data.Doublets.Cli
```

<img width="811" alt="Screenshot 2025-05-16 at 5 48 06 AM" src="https://github.com/user-attachments/assets/615df4ce-658e-4bab-a483-96fae200f106" />

### Rust

```bash
# CLI: build and install the `clink` binary from crates.io.
cargo install link-cli

# Library: pull `link_cli` into your own Cargo project.
cargo add link-cli
```

The NuGet CLI tool is the C# implementation and exposes the complete production
command surface, including persistent transformation triggers. The Rust crate
mirrors the core query engine, named references, LiNo import/export, structure
formatting, and the WebAssembly workbench API. Persistent transformation
trigger CLI options currently exist only in the C# tool.

This tool provides all CRUD operations for links using single [substitution operation](https://en.wikipedia.org/wiki/Markov_algorithm) ([ru](https://ru.wikipedia.org/wiki/Нормальный_алгоритм)) which is turing complete.

Each operations split into two parts:

```
(matching pattern)
(substitution pattern)
```

When match pattern and substitution pattern are essensially the same we get no changes (no operation), it may seem like it does not any write, but it actually does the read operation.

For example when `--changes` option is enabled this operation:

```
((1: 1 1)) ((1: 1 1))
```

will output:

```
((1: 1 1)) ((1: 1 1))
```

That is change of 1-st link with start (source) at itself and end (target) at itself to itself. Meaning no change, but as match pattern applies only to the link with 1 as index, 1 as source and 1 as target, this "no change" can be used as read query.

Creation is just a replacement of nothing to something:

```
() ((1 1))
```

Where first `()` is just empty sequence of links, that symbolizes nothing. And `((1 1))` is a sequence of link with 1 as a start and 1 as end, the index is undefined so it for database to decide actual available id (index).

Deletion is just a replacement of something to nothing:

```
((1 1)) () 
```

Where `((1 1))` is a sequence of match patterns, with a single pattern for a link with 1 as a start and 1 as end, the index is undefined, meaning it can be any index. It will match only existing link, if no such link found there will be no match. Last `()` is just empty sequence of links, that symbolizes nothing. We don't have matched link on the right side, meaning it will be effectively deleted.

And the update is substitution itself, obviously.

```
((1: 1 1)) ((1: 1 2))
```

In that case we have a link with 1-st id on both sides, meaning it is not deleted and not created, it is changed. In this particular example with change the target of the link (its ending) to 2. 2 is ofcourse id of another link. In here we have only links, nothing else.

## Create single link

Create link with 1 as source and 1 as target.

```bash
clink '() ((1 1))' --changes --after
```
→
```
() ((1: 1 1))
(1: 1 1)
```

Create link with 2 as source and 2 as target.

```bash
clink '() ((2 2))' --changes --after
```
→
```
() ((2: 2 2))
(1: 1 1)
(2: 2 2)
```

## Create multiple links

Create two links at the same time: (1 1) and (2 2).

```bash
clink '() ((1 1) (2 2))' --changes --after
```
→
```
() ((2: 2 2))
() ((1: 1 1))
(1: 1 1)
(2: 2 2)
```

## Read all links

```bash
clink '((($i: $s $t)) (($i: $s $t)))' --changes --after
```
→
```
((1: 1 1)) ((1: 1 1))
((2: 2 2)) ((2: 2 2))
(1: 1 1)
(2: 2 2)
```

Where `$i` stands for variable named `i`, that stands for `index`. `$s` is for `source` and `$t` is for `target`.

A short version of read operation will also work:
```
clink '((($i:)) (($i:)))' --changes
```

## Named references

String references can name links. Names are persisted in a companion
`<database-name>.names.links` file and are rendered in output whenever a link
has a name. Missing named references are rejected by default; use
`--auto-create-missing-references` when the query should create missing names as
self-referential point links.

Create a named `child` link from named `father` and `mother` references:

```bash
clink --db family.links --auto-create-missing-references '() ((child: father mother))' --changes --after
```
→
```
((father: 0 0)) ((father: father father))
((mother: 0 0)) ((mother: mother mother))
() ((child: father mother))
(father: father father)
(mother: mother mother)
(child: father mother)
```

Read the named link without changing it:

```bash
clink --db family.links '(((child: father mother)) ((child: father mother)))' --changes
```
→
```
((child: father mother)) ((child: father mother))
```

Update the named link by swapping its source and target:

```bash
clink --db family.links '((child: father mother)) ((child: mother father))' --changes --after
```
→
```
((child: father mother)) ((child: mother father))
(father: father father)
(mother: mother mother)
(child: mother father)
```

Delete the named link:

```bash
clink --db family.links '((child: mother father)) ()' --changes --after
```
→
```
((3: mother father)) ()
(father: father father)
(mother: mother mother)
```

The deleted link no longer has the `child` name when the deletion change is
printed because deleting a link also removes its name mapping.

Variables work with named references too. Starting from a database where `child`
still points to `father mother`, this query reads every link and writes it back
with source and target swapped:

```bash
clink --db family.links '((($index: $source $target)) (($index: $target $source)))' --changes --after
```
→
```
((father: father father)) ((father: father father))
((mother: mother mother)) ((mother: mother mother))
((child: father mother)) ((child: mother father))
(father: father father)
(mother: mother mother)
(child: mother father)
```

If a name contains spaces, parentheses, colons, or quotes, the exporter quotes
it in LiNo output.

## Structure formatting

Use `--structure <id>` to render the left branch of a link recursively. The
formatter keeps the current link index and stops recursion when it would revisit
a link.

```bash
clink --db family.links --structure 3
```
→
```
(child: (father: father father) mother)
```

For numbered links:

```bash
clink --db structure.links --structure 4
```
→
```
(4: (3: (2: (1: 1 1) 2) 1) 2)
```

## Import database from LiNo

Use `--in`, `--import`, or `--lino-input` to read a `.lino` file before the
query runs. Import accepts one complete two-value link definition per non-empty
line.

```lino
(father: father father)
(mother: mother mother)
(child: father mother)
```

```bash
clink --db imported.links --import family.lino --export exported.lino
```

`exported.lino`:

```lino
(father: father father)
(mother: mother mother)
(child: father mother)
```

## Export database as LiNo

Use `--out` or `--export` to write the complete database to a `.lino` file after the query is processed. The older `--lino-output` option is also accepted.

```bash
clink --auto-create-missing-references '() ((child: father mother))' --export database.lino
```

`database.lino`:

```lino
(father: father father)
(mother: mother mother)
(child: father mother)
```

When links do not have names, exported references are plain link numbers:

```lino
(1: 1 1)
(2: 1 2)
```

## Persistent transformation triggers

Store a query as a trigger with `--always` to apply it after later write operations:

```bash
clink --db graph.links --always '(((1: 1 1)) ((1: 1 2)))'
clink --db graph.links --auto-create-missing-references '() ((1: 1 1))' --after
```

Use `--once` for a trigger that deletes itself after the first successful application, and `--never` to remove matching stored triggers:

```bash
clink --db graph.links --once '(((1: 1 1)) ((1: 1 2)))'
clink --db graph.links --never '(((1: 1 1)) ((1: 1 2)))'
```

Triggers are stored as binary links using the structure `(Always ((Condition ...) (Substitution ...)))` or `(Once ((Condition ...) (Substitution ...)))`. By default they are kept in a companion `<database-name>.triggers.links` file, such as `graph.triggers.links` for `graph.links`. Use `--triggers-file path/to/triggers.links` to choose a different companion file, `--triggers` to enable trigger evaluation explicitly, or `--embed-triggers` to store trigger links in the main database file.

## Database files

`--db` selects the primary links database file. With the default database name,
the CLI uses these files:

| File | Purpose |
|------|---------|
| `db.links` | Primary link triples. |
| `db.names.links` | Named-reference sidecar used by `NamedTypesDecorator`. |
| `db.triggers.links` | Persistent transformation trigger sidecar when trigger support is enabled. |

Use `--triggers-file` to point trigger storage somewhere else, or
`--embed-triggers` to store trigger links in the main database. Names are stored
separately so the primary links database remains numeric.

## Update single link

Update link with index 1 and source 1 and target 1, changing target to 2.

```bash
clink '((1: 1 1)) ((1: 1 2))' --changes --after
```
→
```
((1: 1 1)) ((1: 1 2))
(1: 1 2)
(2: 2 2)
```

## Update multiple links

Update link with index 1 and source 1 and target 1, changing target to 2.

```bash
clink '((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1))' --changes --after
```
→
```
((1: 1 1)) ((1: 1 2))
((2: 2 2)) ((2: 2 1))
(1: 1 2)
(2: 2 1)
```

## Delete single link

Delete link with source 1 and target 2:

```bash
clink '((1 2)) ()' --changes --after
```
→
```
((1: 1 2)) ()
(2: 2 2)
```

Delete link with source 2 and target 2:

```bash
clink '((2 2)) ()' --changes --after
```
→
```
((2: 2 2)) ()
```

## Delete multiple links

```bash
clink '((1 2) (2 2)) ()' --changes --after
```
→
```
((1: 1 2)) ()
((2: 2 2)) ()
```

## Delete all links

```bash
clink '((* *)) ()' --changes --after
```
→
```
((1: 1 2)) ()
((2: 2 2)) ()
```

## Link deduplication

When creating nested links, identical sub-links are automatically deduplicated. This means if the same link pattern appears multiple times, it will only be created once and reused.

### Example 1: Duplicate pair deduplication

Create a nested structure where `(m a)` appears twice:

```bash
clink '() (((m a) (m a)))' --after
```
→
```
(m: m m)
(a: a a)
(3: m a)
(4: 3 3)
```

In this example:
- `m` and `a` are named self-referencing links
- `(m a)` is created once with index 3
- The outer link `((m a) (m a))` has index 4, pointing to link 3 twice (source=3, target=3)

### Example 2: Multiple expressions with shared sub-links

```bash
clink '(((m a) (m a))) (((p a) (p a)))' --after
```
→
```
(p: p p)
(a: a a)
(3: p a)
(4: 3 3)
```

The update operation replaces the structure, but note that `a` is reused between expressions.

### Example 3: Different sub-links are not deduplicated

```bash
clink '() (((m a) (a m)))' --after
```
→
```
(m: m m)
(a: a a)
(3: m a)
(4: a m)
(5: 3 4)
```

Since `(m a)` and `(a m)` are different links, they are both created. The outer link references both of them.

## Complete examples:

```bash
clink '() ((1 1) (2 2))' --changes --after

clink '((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1))' --changes --after

clink '((1 2) (2 1)) ()' --changes --after
```

```bash
clink '() ((1 2) (2 1))' --changes --after

clink '((($index: $source $target)) (($index: $target $source)))' --changes --after

clink '((1: 2 1) (2: 1 2)) ()' --changes --after
```

## All options and arguments

The C# NuGet tool supports every option below. The Rust CLI currently supports
the core query, storage, output, import/export, and structure options; trigger
options are C#-only for now.

| Parameter               | Type    | Default Value  | Aliases                             | Description                                                                |
|-------------------------|---------|----------------|-------------------------------------|----------------------------------------------------------------------------|
| `--db`                  | string  | `db.links`     | `--data-source`, `--data`, `-d`     | Path to the links database file                                            |
| `--query`               | string  | _None_         | `--apply`, `--do`, `-q`             | LiNo query for CRUD operation                                              |
| `query` (positional)    | string  | _None_         | _N/A_                               | LiNo query for CRUD operation (provided as the first positional argument)  |
| `--trace`               | bool    | `false`        | `-t`                                | Enable trace (verbose output)                                              |
| `--auto-create-missing-references` | bool | `false` | _None_                              | Create missing numeric and named references as self-referential point links |
| `--structure`           | uint?   | _None_         | `-s`                                | ID of the link to format its structure                                     |
| `--before`              | bool    | `false`        | `-b`                                | Print the state of the database before applying changes                    |
| `--changes`             | bool    | `false`        | `-c`                                | Print the changes applied by the query                                     |
| `--after`               | bool    | `false`        | `--links`, `-a`                     | Print the state of the database after applying changes                     |
| `--in`                  | string  | _None_         | `--import`, `--lino-input`          | Read and import a LiNo file before query execution                         |
| `--out`                 | string  | _None_         | `--export`, `--lino-output`         | Write the complete database as a LiNo file                                 |
| `--always`              | bool    | `false`        | _None_                              | Store the query as an always-on persistent transformation trigger          |
| `--once`                | bool    | `false`        | _None_                              | Store the query as a one-shot persistent transformation trigger            |
| `--never`               | bool    | `false`        | _None_                              | Remove stored persistent transformation triggers matching the query        |
| `--triggers`            | bool    | `false`        | _None_                              | Enable persistent transformation triggers for the command                  |
| `--triggers-file`       | string  | `<db>.triggers.links` | _None_                       | Path to the persistent transformation trigger links database               |
| `--embed-triggers`      | bool    | `false`        | _None_                              | Store persistent transformation triggers in the main links database        |

The query can be passed as the first positional argument or through `--query`,
`--apply`, or `--do`. In the Rust CLI, `--query` takes precedence when both
`--query` and a positional query are provided.

## For developers and debugging

### Execute from root

```bash
dotnet run --project csharp/Foundation.Data.Doublets.Cli -- '(((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1)))' --changes --after
```

### Execute from folder

```bash
cd csharp/Foundation.Data.Doublets.Cli
dotnet run -- '(((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1)))' --changes --after
```

### Complete examples:

```bash
dotnet run --project csharp/Foundation.Data.Doublets.Cli -- '() ((1 1) (2 2))' --changes --after

dotnet run --project csharp/Foundation.Data.Doublets.Cli -- '((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1))' --changes --after

dotnet run --project csharp/Foundation.Data.Doublets.Cli -- '((1 2) (2 1)) ()' --changes --after
```

```bash
dotnet run --project csharp/Foundation.Data.Doublets.Cli -- '() ((1 2) (2 1))' --changes --after

dotnet run --project csharp/Foundation.Data.Doublets.Cli -- '((($index: $source $target)) (($index: $target $source)))' --changes --after

dotnet run --project csharp/Foundation.Data.Doublets.Cli -- '((1: 2 1) (2: 1 2)) ()' --changes --after
```

### Publish next version:

```bash
VERSION=$(awk -F'[<>]' '/<Version>/ {print $3}' csharp/Foundation.Data.Doublets.Cli/Foundation.Data.Doublets.Cli.csproj) && git tag "v$VERSION" && git push origin "v$VERSION"
```

## Running a Specific Test with Detailed Output

To run a specific test (e.g., `DeleteAllLinksByIndexTest`) with detailed output, use:

```
dotnet test --filter "FullyQualifiedName=Foundation.Data.Doublets.Cli.Tests.Tests.AdvancedMixedQueryProcessor.DeleteAllLinksByIndexTest" --logger "console;verbosity=detailed"
```

This will execute only the specified test and show detailed logs in the console.

**Short version:**
```
dotnet test --filter DeleteAllLinksByIndexTest -v n
```
