# link-cli

[![C# CI/CD Pipeline](https://github.com/link-foundation/link-cli/actions/workflows/csharp.yml/badge.svg)](https://github.com/link-foundation/link-cli/actions/workflows/csharp.yml)
[![Rust CI/CD Pipeline](https://github.com/link-foundation/link-cli/actions/workflows/rust.yml/badge.svg)](https://github.com/link-foundation/link-cli/actions/workflows/rust.yml)
[![WebAssembly CI](https://github.com/link-foundation/link-cli/actions/workflows/wasm.yml/badge.svg)](https://github.com/link-foundation/link-cli/actions/workflows/wasm.yml)
[![JS client CI](https://github.com/link-foundation/link-cli/actions/workflows/js-client.yml/badge.svg)](https://github.com/link-foundation/link-cli/actions/workflows/js-client.yml)
[![NuGet (clink)](https://img.shields.io/nuget/v/clink?logo=nuget&label=clink)](https://www.nuget.org/packages/clink)
[![NuGet (library)](https://img.shields.io/nuget/v/Foundation.Data.Doublets.Cli?logo=nuget&label=Foundation.Data.Doublets.Cli)](https://www.nuget.org/packages/Foundation.Data.Doublets.Cli)
[![Crates.io](https://img.shields.io/crates/v/link-cli?logo=rust&label=Crates.io)](https://crates.io/crates/link-cli)
[![Docs.rs](https://docs.rs/link-cli/badge.svg)](https://docs.rs/link-cli)
[![C# Release](https://img.shields.io/github/v/release/link-foundation/link-cli?filter=csharp-v*&label=C%23%20release)](https://github.com/link-foundation/link-cli/releases?q=C%23&expanded=true)
[![Rust Release](https://img.shields.io/github/v/release/link-foundation/link-cli?filter=rust-v*&label=Rust%20release)](https://github.com/link-foundation/link-cli/releases?q=Rust&expanded=true)

`clink` (`CLInk` `cLINK`), a CLI tool to manipulate links using single substitution operation.

It is based on [associative theory](https://github.com/link-foundation/meta-theory/blob/main/archive/0.0.2/article.md) (also in [ru](https://github.com/link-foundation/meta-theory/blob/main/archive/0.0.1/article.md)) and [Links Notation](https://github.com/linksplatform/Protocols.Lino) (also in [ru](https://github.com/linksplatform/Protocols.Lino/blob/main/README.ru.md))

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

- [docs/ROADMAP.md](docs/ROADMAP.md): the dependency order and verification of the associative architecture in doublets and triplets.
- [docs/REQUIREMENTS.md](docs/REQUIREMENTS.md): implemented and planned requirements collected from issues and PR comments.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): repository layout, major components, dependencies, storage files, and CI.
- [docs/COMPOSITION.md](docs/COMPOSITION.md): how the permission, union, remap and external-id layers compose with each other and the other layers, in C#, Rust and JavaScript, with scenarios.
- [docs/TRIPLETS.md](docs/TRIPLETS.md): triplets `(index: type from to)` stored in doublets, the encoding, the alternatives and the cost of every operation.
- [docs/GRAPHQL.md](docs/GRAPHQL.md): the Hasura-compatible GraphQL server for Deep, `clink --serve --graphql`.
- [docs/protocol/](docs/protocol): the links protocol, binary links notation and subscriptions, with the golden conversations every port replays.
- [examples/](examples): runnable examples, one folder per feature.
- [docs/PARITY.md](docs/PARITY.md): the cross-language parity manifest, its checker and the shared golden fixtures.
- [docs/JS-WORKBENCH.md](docs/JS-WORKBENCH.md): what the JavaScript workbench and client implement and what they delegate to the Rust WebAssembly build.
- [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/ci-cd/troubleshooting.md](docs/ci-cd/troubleshooting.md): the CI stages, the single-pusher rule, and how to read a failed check.
- [docs/HOW-IT-WORKS.md](docs/HOW-IT-WORKS.md): deeper explanation of query processing, references, import/export, triggers, and the WebAssembly workbench.
- [docs/case-studies/issue-71/README.md](docs/case-studies/issue-71/README.md): evidence and analysis behind the original documentation refresh.
- [docs/case-studies/issue-92/README.md](docs/case-studies/issue-92/README.md): evidence and analysis behind the dual CLI + library packaging and unified API documentation site.
- [docs/case-studies/issue-94/README.md](docs/case-studies/issue-94/README.md): evidence and analysis for the optional transactions and version-control layers.
- [docs/case-studies/issue-98/README.md](docs/case-studies/issue-98/README.md): evidence and analysis for making the libraries reusable as a doublets-backed transactional store.
- [docs/case-studies/issue-105/README.md](docs/case-studies/issue-105/README.md): requirements, prior art and the wire formats of the LiNo text and binary protocols over TCP.
- [docs/case-studies/issue-108/README.md](docs/case-studies/issue-108/README.md): the JS/TS client, the guarantees of each backend, what conformance found and the synthetic consumer benchmark.
- [docs/case-studies/issue-111/README.md](docs/case-studies/issue-111/README.md): the JavaScript-first CI gate, cross-language parity, branch protection and repository governance.

### API references

- C# library: package page at <https://www.nuget.org/packages/Foundation.Data.Doublets.Cli> and the DocFX-generated reference on [GitHub Pages](https://link-foundation.github.io/link-cli/csharp/).
- Rust library: <https://docs.rs/link-cli> (also mirrored on [GitHub Pages](https://link-foundation.github.io/link-cli/rust/link_cli/)).
- API landing page: <https://link-foundation.github.io/link-cli/docs/> (the site root hosts the WebAssembly workbench).

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

Both implementations expose the same command surface, so a scenario written
against one runs unchanged against the other: the query engine, named
references, LiNo import/export, structure formatting, persistent
transformation triggers, transactions and version control. The Rust crate adds
the WebAssembly workbench API on top.

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

Quoted references in queries always mean names in both C# and Rust (including
WebAssembly). For example, `() ((p: x "0"))` with
`--auto-create-missing-references` creates a point named `0` and uses it as the
target of `p`. `"1"`, `'1'`, and `` `1` `` refer to the name `1`; bare `1` still
refers to address 1. Quoted numeric identifiers also work in definitions,
restrictions, updates, deletions, and stored triggers. Without automatic creation,
the named link must already exist or be defined by the substitution. Reference
literals such as `~1{31}` likewise mean names. Quoting `*` or `$value` makes it a
literal name.

Create a named `child` link from named `father` and `mother` references:

```bash
clink --db family.links --auto-create-missing-references '() ((child: father mother))' --changes --after
```
→
```
() ((father: father father))
() ((mother: mother mother))
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

If a name contains spaces, parentheses, colons, quotes, or backslashes, the C#
exporter quotes it in LiNo output. Names containing control characters or both
quote kinds use lossless reference literals, as described below.

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
line. In C#, a quoted name can span physical lines.

Both implementations parse and validate the entire LiNo input before changing
links or names. A malformed file leaves an empty or populated database unchanged
and the CLI exits with code 1; no `--transactions` flag is needed. This validation
also applies to the C# `LinoDatabaseInput.ImportText`/`ReadFromFile` and Rust
`import_lino_text`/`import_lino_file` library APIs. Storage failures or process
termination during writes retain the underlying store's durability guarantees.

```lino
(father: father father)
(mother: mother mother)
(child: father mother)
```

```bash
clink --db imported.links --import family.lino --export exported.lino
```

The **C# importer treats backslashes literally** inside single, double, or
backtick quotes. For example, `'x\y'`, `'C:\Users\name\'`, and `'literal\n'`
keep every backslash; `\n` is two characters, not an escape for a newline. Choose
a quote delimiter absent from the name, or use a lossless reference literal:

```lino
(path: path 'C:\Users\name\')
(mixed: mixed `both'"quotes`)
(multiline: multiline ~1{610d0a62})
```

`~1{610d0a62}` represents `a`, CRLF, then `b`: the braces hold the exact UTF-8
bytes as hexadecimal. The C# importer and exporter share this literal format
with the LiNo protocol library. Unsupported versions, malformed hexadecimal,
and invalid UTF-8 are rejected. A quoted `'~1{31}'` is the literal name
`~1{31}`, while bare `~1{31}` decodes to the name `1`. Quoted and encoded numeric
names remain distinct from bare link addresses.

The C# exporter uses these literals for control characters (including newlines)
and names containing both single and double quotes. Exported definitions stay
on one physical line and can be imported into a fresh database without changing
the names. Existing text exports that used backslashes to escape quotes should
be rewritten using another quote delimiter or a reference literal.

For large **Rust** named imports, opt into lookup-index decorators:

```bash
clink --db imported.links --indexed-lookups --import family.lino --export exported.lino
```

Indexes trade O(n) additional memory for indexed pair/source/target lookups.
Omit `--indexed-lookups` to retain the original scan algorithms. See the
[Rust performance and limitations notes](rust/README.md#optional-lookup-indexes-for-named-imports)
and [scaling benchmark](examples/named-import/README.md).

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

## Export and import a binary store archive

Use `--export-binary` (or `--binary-output`, `--binary-out`) to write the
complete database, names included, as a store archive in
[binary links notation](docs/protocol/binary-links-notation.md#10-store-archive).
Every link keeps its address, holes included. `--import-binary` (or
`--binary-input`, `--binary-in`) reads an archive back before `--in` and the
query run:

```bash
clink --db family.links --export-binary family.bin
clink --db copy.links --import-binary family.bin --after
```

For the family above the archive takes 36 bytes, against 71 bytes of LiNo.
Both ports write and read the same bytes, so an archive written by the Rust
CLI can be read by the C# CLI and the other way around
([examples/archive](examples/archive/README.md)).

## Type hierarchy

A type hierarchy `Type -> SubType -> … -> Value` of any depth, with multiple
inheritance, is kept in ordinary links, so the CLI needs no new syntax:

- the type of a link is its source: `(v: S x)` is a value of type `S`;
- the root is the point `(Type: Type Type)`;
- a sub-type of `T` is `(S: T S)`, a value of `T` that points at itself;
- an extra parent is a pair `(p: S T2)` marked by `(m: Extends p)`, where
  `Extends` is a type too.

Build `Type -> Doc -> Note -> News`, make `News` extend `Public` as well, and
add a value `today` of `News`:

```bash
clink --db types.links --auto-create-missing-references '() ((Type: Type Type) (Extends: Extends Extends) (User: Type User) (Doc: Type Doc) (Public: Type Public) (Note: Doc Note) (News: Note News) (Extends (News Public)) (today: News today))' --after
```
→
```
(Type: Type Type)
(Extends: Extends Extends)
(User: Type User)
(Doc: Type Doc)
(Public: Type Public)
(Note: Doc Note)
(News: Note News)
(8: News Public)
(9: Extends 8)
(today: News today)
```

The links that point at themselves are the types (`today` is one too: a
value `(v: T v)` can be sub-typed further), and the links whose source is
`News` are its values and its extra parents:

```bash
clink --db types.links '((($t: $p $t)) (($t: $p $t)))' --changes
clink --db types.links '((($v: News $x)) (($v: News $x)))' --changes
```
→
```
((Type: Type Type)) ((Type: Type Type))
((Extends: Extends Extends)) ((Extends: Extends Extends))
((User: Type User)) ((User: Type User))
((Doc: Type Doc)) ((Doc: Type Doc))
((Public: Type Public)) ((Public: Type Public))
((Note: Doc Note)) ((Note: Doc Note))
((News: Note News)) ((News: Note News))
((today: News today)) ((today: News today))
((8: News Public)) ((8: News Public))
((today: News today)) ((today: News today))
```

Walking the hierarchy is the job of `TypesDecorator`, a decorator over the
links in both libraries (C# `TypesDecorator<TLinkAddress>`, which implements
`ITypes<TLinkAddress>`; Rust `link_cli::TypesDecorator`, which implements the
`Types` trait). It answers `TypeOf`, `IsType`, `ParentsOf`, `AncestorsOf`,
`IsA` (the type itself or of it) and `IsOf` (strictly of it) through any
depth, diamonds and cycles, and writes with `Define(parent, more…)`,
`Extend(type, parent)` and `Instance(type[, target])`:

```csharp
var types = new TypesDecorator<uint>(namedLinks); // finds or creates Type and Extends
var doc = types.Define(types.Root);
var @public = types.Define(types.Root);
var note = types.Define(doc);
var news = types.Define(note, doc, @public);
types.IsA(types.Instance(news), @public); // true
```

The pinned types `(1: 1 1)`, `(2: 1 2)`, `(3: 1 3)`, … are sub-types of the
root `1`, so a store with pinned types is already such a hierarchy. Runnable
scripts for both CLIs and a Rust example live in
[`examples/type-hierarchy/`](examples/type-hierarchy).

## Triplets

deep-foundation's links are triplets `(index: type from to)`. Both libraries
keep them in any doublets store with `TripletsOverDoublets` (C#
`TripletsOverDoublets<TLinkAddress>`, which implements `ITriplets<TLinkAddress>`;
Rust `link_cli::TripletsOverDoublets`, which implements `Triplets`). A node
`(i: T 0 0)` is the doublet `(i: T i)` and a link `(i: T f t)` is
`(i: T ((Pair f) t))`, so the type stays the source and the
[type hierarchy](#type-hierarchy) reads triplets as values of their types.
Updates keep the address, deletes cascade like doublet deletes, and wrappers
and pairs nothing uses are collected.

```csharp
var triplets = new TripletsOverDoublets<uint>(namedLinks); // finds or creates Pair and Nothing
TripletsLino.Import(triplets, "(knows: Type 0 0)\n(alice: Type 0 0)\n(bob: Type 0 0)\n(carol: Type 0 0)\n(Type: Type 0 0)\n(r: knows alice bob)");
TripletsQueryProcessor.ProcessQuery(triplets, "((($r: knows alice bob)) (($r: knows alice carol)))");
triplets.EachTriplet(TripletQuery<uint>.ByType(triplets.GetByName("knows"))); // [(r: knows alice carol)]
Console.Write(TripletsLino.Export(triplets));
```

The encoding, the alternatives that were measured and the cost of `Each` by
type, from and to and of deletion are in [docs/TRIPLETS.md](docs/TRIPLETS.md);
the Rust example is in [`examples/triplets/`](examples/triplets).

`clink --serve --triplets` serves them over the
[text and binary protocol](#serving-triplets).

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

## Use as an embedded store

Both libraries can be embedded as a transactional links store instead of being
driven through the CLI. The transactions layer is written against a storage
abstraction rather than one concrete store, so an application can supply its
own:

- **Any address type.** Rust's `GenericTransactionsDecorator<T, S, L>` and C#'s
  `TransactionsDecorator<TLinkAddress>` are generic over the doublets address
  type (`u32`/`u64`/`usize`, `uint`/`ulong`). The transitions wire format writes
  addresses in decimal, so a log written by a narrow store reads back unchanged
  in a wider one, and an address that does not fit is rejected rather than
  truncated.
- **A real doublets store.** Rust's `storage::DoubletsStorage` wraps a
  file-mapped `doublets::unit::Store`, or adopts one the caller already owns.
  Its links are mutated **in place**, so the inode never changes and other
  processes that mapped the same file keep observing the same data.
- **Stated durability.** Writes to a file-mapped store survive a *process*
  crash with no explicit save; surviving a *machine* crash needs the `fsync`
  that `flush()` performs. Crash recovery replays committed-but-unapplied
  transitions and rolls back uncommitted ones when the store is reopened.
- **Multi-process access.** Both implementations lock a `<database>.lock`
  sidecar — shared for readers, exclusive for writers — and expose a cheap
  fingerprint that answers "has anyone else written since I last looked?".

Runnable demos for both languages live in
[`examples/embedded-store/`](examples/embedded-store).

See [rust/README.md](rust/README.md#use-as-a-library),
[csharp/README.md](csharp/README.md#use-as-a-library), and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#embedding-the-library) for the
details.

## Serve a database over TCP

`--serve` exposes a database over TCP. `--connect` sends one query to a
served database and prints the reply:

```bash
clink --db db.links --serve 127.0.0.1:7777          # terminal 1
clink --connect 127.0.0.1:7777 '() ((1 1))'         # terminal 2
() ((1: 1 1))
clink --connect 127.0.0.1:7777                      # no query: list every link
(1: 1 1)
```

A request is a substitution query, so it covers create, read, update and
delete. The reply has one `(before) (after)` line per change, the same as
`--changes`. A failed query gets `(error: 'message')` back.

There are two interchangeable protocols:

- **text**: UTF-8 LiNo. Each message ends with a line holding a single `.`,
  so you can also talk to the server with `nc`.
- **binary**: `--protocol binary`. Each packet is
  [binary links notation](docs/protocol/binary-links-notation.md): it counts
  its links first, and references are 8, 16, 32 or 64 bits wide depending on
  the largest one. Three optional decorators can each be switched on
  independently:
  - `--external-references`: numbers and characters travel as Hybrid external
    references.
  - `--arity <RANGE>`: the link lengths the encoder may use. The default, `2`,
    sends only doublets. `2..3` adds triplets, and `1..` allows links of any
    length.
  - `--packed-widths`: each section of the packet gets the narrowest width its
    links need.

The server detects the protocol of each message and replies in the same one.
C# and Rust servers and clients work with each other: see
[`examples/tcp/`](examples/tcp) and
[docs/case-studies/issue-105](docs/case-studies/issue-105/README.md).

Ctrl+C or `SIGTERM` stops a server gracefully: it closes the store and exits
with status 0, so it can run under a service manager or in a container.

### Serve over any IPC mechanism

TCP is one of many transports. Every `--serve`, `--connect` and `--forward`
address is an endpoint of
[IPC transports v1](docs/protocol/ipc-transports-v1.md), and every endpoint
carries both protocols between the Rust and C# CLIs, their libraries and the
[JavaScript client](js/client/README.md#ipc-endpoints):

| Endpoint | Mechanism | Platforms |
| --- | --- | --- |
| `127.0.0.1:7777`, `tcp:HOST:PORT` | TCP/IP loopback (or network) | all |
| `udp:127.0.0.1:7777` | UDP loopback with acknowledged, ordered delivery | all |
| `unix:/tmp/links.sock` | Unix domain socket | Linux, macOS, Windows |
| `pipe:NAME` | named pipe (on POSIX: a pair of FIFOs in a directory) | all |
| `fifo:DIR` | two POSIX named pipes | Linux, macOS |
| `stdio`, `exec:COMMAND` | anonymous pipes of a child process | all |
| `shm:DIR` | shared memory | all |
| `mmap:DIR` | memory-mapped file | all |
| `file:DIR` | a file on disk, positioned reads and writes only | all |
| `signal:DIR` | memory-mapped file woken by OS signals (Windows: events) | all |
| `mq:DIR` | POSIX message queues | Linux |
| `sysvmq:DIR` | System V message queues | Linux, macOS |
| `mailslot:DIR`, `copydata:DIR`, `alpc:DIR` | mailslots, `WM_COPYDATA`, ALPC | Windows |

The transports are decorators under the protocol, so they are optional,
interchangeable and stack. `--serve` repeats to serve one store on several
endpoints at once, and `--forward` relays every connection of its `--serve`
endpoints to another endpoint, byte for byte, without opening a database:

```bash
clink --db db.links --serve 127.0.0.1:7777 --serve shm:/tmp/links   # terminal 1
clink --serve mq:/tmp/links-queue --forward shm:/tmp/links          # terminal 2: a relay
clink --connect mq:/tmp/links-queue --protocol binary '() ((1 1))'  # terminal 3
() ((1: 1 1))
clink --connect 'exec:clink --db db2.links --serve stdio'           # no listener at all
```

`--trace` (or `CLINK_IPC_TRACE=1`) logs each transport event to standard
error. See [`examples/ipc/`](examples/ipc/README.md) for the runnable
examples and the interop check of every kind between every implementation.

### Serving triplets

`--triplets` makes a server read every request as
[triplets](docs/TRIPLETS.md#protocol), with subscriptions and `--permissions`
applying to them too. A server without it keeps serving doublets, because a
request such as `(delete: 3)` cannot say by its shape which one it means.

```bash
clink --db db.links --serve 127.0.0.1:7777 --triplets       # terminal 1
clink --connect 127.0.0.1:7777 '(create: (0 0 0))'          # terminal 2: a point
() ((3: 3 0 0))
clink --connect 127.0.0.1:7777 '(create: (3 0 0))'
() ((4: 3 0 0))
clink --connect 127.0.0.1:7777 '(each: (* 3))'              # each by type
(3: 3 0 0)
(4: 3 0 0)
```

With `--protocol binary --arity 2..3`, a triplet travels as one link of three
references ([binary links notation](docs/protocol/binary-links-notation.md#73-triplets)).
In the libraries, `RemoteLinks` is a `Triplets` store (`ITriplets` in C#) behind
such a server, and
[`@link-foundation/link-cli-client`](js/client/README.md#triplets) has the same
calls.

### Use a served database as a store

In both libraries, `RemoteLinks` is a links store whose links live behind a
server. It implements the same interfaces as a local store: `INamedTypesLinks`
in C#, and `Links`, `Doublets` and `NamedTypeLinks` in Rust. So code written
against them, the query processor included, switches from a local file to a
server by swapping one value:

```csharp
static void Run(INamedTypesLinks<uint> links) =>
    AdvancedMixedQueryProcessor.ProcessQuery(links, new() { Query = "() ((1 1))" });

using (var local = new NamedTypesDecorator<uint>("db.links")) Run(local);
using (var remote = RemoteLinks.Connect("127.0.0.1:7777", new TextLinoProtocol())) Run(remote);
```

```rust
fn run(store: &mut impl NamedTypeLinks) -> anyhow::Result<()> {
    QueryProcessor::new(false).process_query(store, "() ((1 1))")?;
    Ok(())
}

run(&mut NamedTypesDecorator::new("db.links", false)?)?;
run(&mut RemoteLinks::connect("127.0.0.1:7777", TextLinoProtocol::new())?)?;
```

Each call is one request, such as `(count: (* 1))`, `(update: (1 2 2))` or
`(set-name: (2 'a pair'))`. A write replies with the net change of every link
it touched. [`docs/protocol/links-operations.txt`](docs/protocol/links-operations.txt)
records one whole conversation, and both ports replay it in their tests.
C# also exposes `RemoteLinks<T>`, `LinksOperation<T>` and selected-address server
overloads while retaining the uint APIs. `LinksClient.QueryLossless` carries
original query text to supporting peers; use a checked causal `QueryLinks`
adapter to stage full-u64 edits before explicit session publication. See
[ordinary causal sessions](docs/ENGINE-CAUSAL-SESSIONS.md) for bounds and costs.

### Subscribe to changes

A connection can ask the server to push it every change that fits a
pattern. `(subscribe: (* * *))` replies `(subscription: 1)`. After that,
whenever another connection changes a link that fits, the server sends the
subscriber `(notify: 1)` followed by one `(before) (after)` line for each
change, without being asked:

```
A> (subscribe: (* * *))
A< (subscription: 1)
B> (create: (0 0))
B< () ((1: 0 0))
A< (notify: 1)
A< () ((1: 0 0))
```

- **Numbering.** Subscription numbers count from 1 on each connection.
- **Patterns.** The restriction is the same kind of pattern as in
  `(each: ...)`.
- **Own writes.** A connection is not told about its own writes.
- **Partial matches.** A side of a change that does not fit the pattern is
  left out, so a link that stops matching arrives as a deletion.
- **Hidden links.** The server's readable predicate hides links from
  subscribers in the same way: `LinksServerOptions.Readable` in C#,
  `LinksServer::with_readable` in Rust.
- **Stopping.** `(unsubscribe: 1)` replies nothing, and closing the
  connection drops its subscriptions.
- **Slow readers.** A subscriber that lets more than 1024 pushes pile up is
  disconnected. Change the limit with `MaxPendingNotifications` or
  `max_pending_notifications`.

`RemoteLinks` (`Subscribe`, `Unsubscribe` and `ReceiveNotification`) and the JS
client (`subscribe`, `unsubscribe`, `receiveNotification` and
`onNotification`) keep pushes apart from replies.
[`docs/protocol/subscriptions.txt`](docs/protocol/subscriptions.txt) records a
whole conversation over two connections, and C#, Rust and the JS client replay
it. To see it run, use
[`examples/tcp/watch-changes.mjs`](examples/tcp/watch-changes.mjs).

### Use a served database from JavaScript or TypeScript

[`@link-foundation/link-cli-client`](js/client/README.md) is the JS/TS
counterpart of `RemoteLinks`. It speaks the same protocol to `clink --serve`
from Node.js, Bun and Deno, can start and stop a server process of its own,
and serves the in-process WebAssembly store (browsers included) behind the
same contract. Addresses are `BigInt`, and every backend states its
guarantees in `client.capabilities` instead of emulating missing ones:

```js
import { connectTcp } from '@link-foundation/link-cli-client/tcp';

const links = await connectTcp('127.0.0.1:7777');
const link = await links.create(1n, 1n);
await links.setName(link.index, 'a pair');
await links.close();
```

### Serve GraphQL for Deep

`clink --db deep.links --serve 127.0.0.1:3006 --graphql --deep-package fresh.json`
answers as Hasura does for Deep, so `DeepClient` and DeepCase connect to it. See
[docs/GRAPHQL.md](docs/GRAPHQL.md).

## Permissions

`--permissions` checks every read and write against rules stored as links.
The rules live in the data (`--permissions self`) or in a links file of their
own (`--permissions rules.links`); both behave the same. Without the option
nothing is checked and clink works exactly as before.

- A rule is a pair `(g: Subject Object)` marked by `(r: AllowRead g)`,
  `(r: AllowWrite g)` or `(r: AllowReference g)`. `Subject` and `Object` are
  types of the [type hierarchy](#type-hierarchy), or single links. `Anything`
  stands for every subject or every link. Write implies read.
- `--subject` names the subject queries run as, by index or by name. Without
  it the subject is `0`, a guest, which only rules about `Anything` let in.
- A link the subject created is its own while its source is the null link,
  so it can still be given a type it may not read.
- A link the subject may not read is missing to it. A create, update or
  delete it may not do fails with `Forbidden: <operation> <link>`, and the
  whole query is undone, including the writes the store made on its own.
- A reference is a right of its own: a link the subject may not refer to answers
  exactly as a missing one, and its name as an unknown name. A subject may refer
  to a link it may read, to itself, to what an `AllowReference` rule covers
  (`Object` and the links of type `Object`), and to the `Object` of its own
  `AllowRead` and `AllowWrite` rules, so `User writes Note` is enough to type a
  link as `Note`. `AllowReference` does not allow reading. A reference a link
  already holds may stay, and choosing the address of a new link is a reference
  too. A rule `Anything` to `Anything` lets every subject refer to everything.

```bash
clink --db data.links --auto-create-missing-references '() ((Type: Type Type) (Extends: Extends Extends) (AllowRead: AllowRead AllowRead) (AllowWrite: AllowWrite AllowWrite) (AllowReference: AllowReference AllowReference) (Anything: Anything Anything) (User: Type User) (Admin: User Admin) (Doc: Type Doc) (Public: Type Public) (Note: Doc Note) (Secret: Doc Secret) (alice: User alice) (boss: Admin boss) (note: Note note) (secret: Secret secret) (news: Public news) (AllowWrite (Admin Anything)) (AllowWrite (User Note)) (AllowRead (User Public)))'
clink --db data.links --permissions self --subject alice --after
```
→
```
(note: Note note)
(news: Public news)
```

```bash
clink --db data.links --permissions self --subject alice '((note: Note note)) ((note: Secret note))'
```
→
```
Error: Query error: Forbidden: update 14
```

A served store checks the subject of each connection. A connection starts as
`--subject` (a guest by default). `(auth: token)` switches it to the subject
the token table of `--tokens` gives the token, and the reply is
`(subject: i)`. The token table is made of links, kept in the data
(`--tokens self`) or in a links file of its own. `--grant-token` and
`--revoke-token` write it, and `--connect --token` authenticates before
sending the query:

```bash
clink --db data.links --tokens tokens.links --subject alice --grant-token alice-secret
clink --db data.links --serve 127.0.0.1:7777 --permissions self --tokens tokens.links   # terminal 1
clink --connect 127.0.0.1:7777 '(count: ())'                          # (count: 0), a guest
clink --connect 127.0.0.1:7777 --token alice-secret '(count: ())'     # (count: 2)
```

In both libraries the layer is `PermissionsDecorator` over any store, with
`PermissionPolicy` reading the rules. The server's sessions take the
authentication as a callback, so a token table in links (`LinksTokenTable`)
is only one choice. Runnable scripts for both CLIs live in
[`examples/permissions/`](examples/permissions), and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#permissions) explains the design.

## Join and remap networks

Several links networks can be read as one, and a network can be read under
other addresses. Each of these layers is a links network itself, so it wraps
a local store, a served one, or another layer, in C#, Rust and JavaScript:

- **Union** (`UnionLinksDecorator`, `UnionLinks`) joins networks, for example a
  client's links with a server's. The first member that has an address
  shadows the rest. A new link goes to the first member, and a write goes to
  the member that has the link.
- **Remap** (`RemapLinksDecorator`, `RemapLinks`) maps addresses both ways. An
  offset moves a server's links out of the client's way, and a table gives
  another system's ids addresses of our own. In JavaScript, a network can be
  read under UUIDs.
- **External ids** (`LinksExternalIds`) keep the ids other systems give our
  links as links, in the data or in a store of their own.
- **Permissions** compose with all of them. Over a union, the rules restrict
  every member. Under it, they restrict one member, such as a server that
  enforces its own rules.

```js
const links = new UnionLinks([local, new RemapLinks(server, new OffsetAddressMap(1n << 24n))]);
const byUuid = new RemapLinks(links, new ExternalIdMap(ids, 'uuid'));
```

[`examples/composition/`](examples/composition) runs a client joined to a
server with `--permissions`, under UUIDs, with both CLIs.
[docs/COMPOSITION.md](docs/COMPOSITION.md) shows how the layers compose and
the scenarios they serve.

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

Both the C# NuGet tool and the Rust CLI support the options below unless marked Rust only.

| Parameter               | Type    | Default Value  | Aliases                             | Description                                                                |
|-------------------------|---------|----------------|-------------------------------------|----------------------------------------------------------------------------|
| `--db`                  | string  | `db.links`     | `--data-source`, `--data`, `-d`     | Path to the links database file                                            |
| `--query`               | string  | _None_         | `--apply`, `--do`, `-q`             | LiNo query for CRUD operation                                              |
| `query` (positional)    | string  | _None_         | _N/A_                               | LiNo query for CRUD operation (provided as the first positional argument)  |
| `--trace`               | bool    | `false`        | `-t`                                | Enable trace (verbose output), including each IPC transport event of `--serve`, `--connect` and `--forward` (as `CLINK_IPC_TRACE=1` does) |
| `--auto-create-missing-references` | bool | `false` | _None_                              | Create missing numeric and named references as self-referential point links |
| `--structure`           | uint?   | _None_         | `-s`                                | ID of the link to format its structure                                     |
| `--before`              | bool    | `false`        | `-b`                                | Print the state of the database before applying changes                    |
| `--changes`             | bool    | `false`        | `-c`                                | Print the changes applied by the query                                     |
| `--after`               | bool    | `false`        | `--links`, `-a`                     | Print the state of the database after applying changes                     |
| `--indexed-lookups`     | bool    | `false`        | _None_                            | Rust only: compose optional indexes for local data and names stores        |
| `--in`                  | string  | _None_         | `--import`, `--lino-input`          | Read and import a LiNo file before query execution                         |
| `--out`                 | string  | _None_         | `--export`, `--lino-output`         | Write the complete database as a LiNo file                                 |
| `--import-binary`       | string  | _None_         | `--binary-input`, `--binary-in`     | Read and import a store archive in binary links notation before `--in`    |
| `--export-binary`       | string  | _None_         | `--binary-output`, `--binary-out`   | Write the complete database, names included, as a binary store archive    |
| `--always`              | bool    | `false`        | _None_                              | Store the query as an always-on persistent transformation trigger          |
| `--once`                | bool    | `false`        | _None_                              | Store the query as a one-shot persistent transformation trigger            |
| `--never`               | bool    | `false`        | _None_                              | Remove stored persistent transformation triggers matching the query        |
| `--triggers`            | bool    | `false`        | _None_                              | Enable persistent transformation triggers for the command                  |
| `--triggers-file`       | string  | `<db>.triggers.links` | _None_                       | Path to the persistent transformation trigger links database               |
| `--embed-triggers`      | bool    | `false`        | _None_                              | Store persistent transformation triggers in the main links database        |
| `--serve`               | string  | _None_         | _None_                              | Serve the database on an endpoint: `host:port` (port `0` picks a free port) or `KIND:ADDRESS` of any [IPC transport](docs/protocol/ipc-transports-v1.md); repeat it to serve one store on several endpoints |
| `--forward`             | string  | _None_         | _None_                              | With `--serve`: relay every connection, byte for byte, to this endpoint instead of serving a database |
| `--connect`             | string  | _None_         | _None_                              | Send the query to a `clink --serve` server on any endpoint (`host:port`, `shm:/tmp/links`, ...) and print the reply |
| `--protocol`            | string  | detected       | _None_                              | `text` or `binary` (with `--serve`, also `any`, which is the default)     |
| `--external-references` | bool    | `false`        | _None_                              | Binary protocol: send numbers and characters as external references       |
| `--arity`               | string  | `2`            | _None_                              | Binary protocol: link lengths to use, `n`, `min..max` or `min..`          |
| `--packed-widths`       | bool    | `false`        | _None_                              | Binary protocol: give each section the narrowest reference width          |
| `--triplets`            | bool    | `false`        | _None_                              | With `--serve`: read every request as triplets, see [Serving triplets](#serving-triplets) |
| `--permissions`         | string  | _None_         | _None_                              | Check reads and writes against rules stored as links: `self` or a rules file |
| `--subject`             | string  | `0` (guest)    | _None_                              | The subject (link index or name) of queries and of served connections     |
| `--tokens`              | string  | _None_         | _None_                              | The token table of `(auth: token)`: `self` or a links file                 |
| `--grant-token`         | string  | _None_         | _None_                              | Let a token authenticate as `--subject` in the `--tokens` table, then exit |
| `--revoke-token`        | string  | _None_         | _None_                              | Forget a token of the `--tokens` table, then exit                          |
| `--token`               | string  | _None_         | _None_                              | With `--connect`: authenticate with `(auth: token)` before the query       |

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
