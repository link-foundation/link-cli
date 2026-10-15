# Named LiNo import scaling (issue #110)

Build once, then compare the original scans with the optional lookup decorator:

```sh
CARGO_BUILD_JOBS=1 cargo build --manifest-path rust/Cargo.toml --release --example named_import_scaling
python3 examples/named-import/run-scaling.py
```

The runner uses numeric and named self-links at 100 / 1,000 / 10,000 links,
with a 25-second deadline for each child. Named identifiers are `atom-` plus
64 digits, as in the issue. Every child uses a fresh temporary store and
cleans it up when it exits normally. Timed-out children may leave a temporary
`/tmp/.tmp*` directory; the runner kills and waits for them before continuing.
The fixture rejects counts above 10,000.

CSV columns: mode, count, bytes, parser milliseconds, import milliseconds
(including parsing), save milliseconds. The parser probe uses the same input
but does not mutate a store. Import timing includes normalization, parsing and
name/link operations. Save timing includes both data and names sidecars.
Build time is excluded. Timings vary by host and build profile.

Run a single case:

```sh
cargo run --manifest-path rust/Cargo.toml --release --example named_import_scaling -- named 10000 indexed
```

Automated regressions in `rust/tests/cli_indexed_import_tests.rs` import all
six fixtures, repeat the import in a fresh process, and restore a binary
archive into another store. They compare the complete exported graph. Each
child has a loose 60-second limit to accommodate debug CI builds; the release
benchmark targets the issue's 25-second bound. Unicode/reserved names and
mutation parity are covered in `rust/tests/indexed_links_tests.rs`.

[Investigation and measurements](../../docs/case-studies/issue-110/README.md).
