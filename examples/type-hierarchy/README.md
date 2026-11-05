# Type hierarchy (issue #148)

A type hierarchy with sub-types and multiple inheritance needs no new CLI
syntax: it is a handful of ordinary links.

- The type of a link is its source: `(v: S x)` is a value of type `S`.
- The root is the point `(Type: Type Type)`.
- A sub-type of `T` is `(S: T S)`.
- An extra parent is a pair `(p: S T2)` marked by `(m: Extends p)`.

`clink-queries.sh` builds `Type -> Doc -> Note -> News`, makes `News` extend
`Public` as well, and then reads the types, the values of `News`, and the
`Extends` marks back. Pass the CLI to use:

```sh
cargo build --manifest-path rust/Cargo.toml
examples/type-hierarchy/clink-queries.sh rust/target/debug/clink

dotnet build csharp/Foundation.Data.Doublets.Cli
examples/type-hierarchy/clink-queries.sh csharp/Foundation.Data.Doublets.Cli/bin/Debug/net10.0/clink
```

Both CLIs print [expected-output.txt](expected-output.txt).

The library walks the hierarchy for you: `TypesDecorator` (C#
`Foundation.Data.Doublets.Cli.TypesDecorator<TLinkAddress>`, Rust
`link_cli::TypesDecorator`) answers `TypeOf`, `IsType`, `ParentsOf`,
`AncestorsOf`, `IsA` and `IsOf` without looping on cycles, and writes with
`Define` and `Instance`. See the Rust example:

```sh
cargo run --manifest-path rust/Cargo.toml --example type_hierarchy
```
