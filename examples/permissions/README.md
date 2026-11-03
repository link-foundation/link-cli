# Permissions stored as links (issue #149)

`--permissions` makes clink check every read and write against rules that
are ordinary links, kept in the data (`--permissions self`) or in a links
file of their own (`--permissions rules.links`). Without the option nothing
is checked, exactly as before.

- A rule is a pair `(g: Subject Object)` marked by `(r: AllowRead g)` or
  `(r: AllowWrite g)`. `Subject` and `Object` are types of the
  [type hierarchy](../type-hierarchy) (or single links); `Anything` stands
  for every subject or every link.
- Write implies read.
- A link a subject created is its own (it may read and write it) while its
  source is the null link, so it can still be given a type it may not read.
- A link the subject may not read is missing to it: queries do not match it
  and `--after` does not print it. A create, update or delete it may not do
  fails with `Forbidden: <operation> <link>`, and the whole query is undone.

`clink-queries.sh` writes a small world (users and admins, notes, secrets and
public links) with three rules, then reads and writes it as `alice`, as a
guest and as `boss`:

```sh
cargo build --manifest-path rust/Cargo.toml
examples/permissions/clink-queries.sh rust/target/debug/clink

dotnet build csharp/Foundation.Data.Doublets.Cli
examples/permissions/clink-queries.sh dotnet csharp/Foundation.Data.Doublets.Cli/bin/Debug/net10.0/clink.dll
```

Both CLIs print [expected-output.txt](expected-output.txt).

`serve-with-tokens.sh` grants `alice` a token in a token table of its own
(`--tokens tokens.links --grant-token`), serves the store with
`--permissions self --tokens tokens.links`, and connects as a guest, as
`alice` (`--token alice-secret`, which sends `(auth: alice-secret)` first)
and with a wrong token. Both CLIs print
[expected-serve-output.txt](expected-serve-output.txt).
