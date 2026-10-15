# Issue #110: named import scaling

[Issue](https://github.com/link-foundation/link-cli/issues/110) ·
[Pull request](https://github.com/link-foundation/link-cli/pull/122)

## Reproduction and cause

The synthetic fixture uses the same doublet topology with either numeric
addresses or `atom-` plus 64 digits as names. It contains no account data.
The unmodified release build imported 1,000 named links in 10,154.913 ms;
parsing alone took 6.762 ms. At 10,000 names a bounded probe exhausted its
30-second CPU allowance (SIGKILL). Thus parsing cannot explain the gap.

Tracing the source explains the growth: every `get_by_name` constructs a new
`NamedLinks` / `UnicodeStringStorage`, names six pinned types, and converts
the requested name into UTF-16 symbol and balanced-sequence doublets.
`LinkStorage::get_or_create` searches the entire map for every doublet.
Name-pair queries and upstream uniqueness/usages lookups also scan the map.
The local conversion caches are discarded at the end of each facade call.
Repeated name-to-address caching alone would leave the scans for new names.

## Optional solution

`IndexedLinksDecorator` wraps the map with pair/source/target indexes.
`with_indexed_lookups()` on the two storage APIs composes it explicitly;
Rust `--indexed-lookups` enables it for local data, names and transaction/VC
sidecars. The default scan path remains available for direct comparisons.
No persisted format changes or additional dependencies are required.

The indexes update at raw insert/remove/clear boundaries, so both inherent
operations and upstream decorators keep them valid. Duplicate pairs retain
all addresses and choose the minimum for search. Raw exact queries preserve
zero/external values; the doublets bridge separately preserves upstream
null, wildcard and repeated-usage semantics. Reload clears and repopulates
the indexes, including after a partially failed load.

## Measurements

Linux x86_64, Rust 1.99.0, release build, one compiler job. Independent parser
and import measurements have normal scheduling variance. Milliseconds:

| Mode | Links | Parser (scan run) | Import with scans | Import with indexes |
|---|---:|---:|---:|---:|
| Numeric | 100 | 0.446 | 0.923 | 0.501 |
| Numeric | 1,000 | 7.099 | 17.917 | 3.920 |
| Numeric | 10,000 | 68.476 | 1,440.385 | 63.202 |
| Named | 100 | 0.511 | 147.067 | 8.712 |
| Named | 1,000 | 6.762 | 10,154.913 | 97.300 |
| Named | 10,000 | — | exceeded 30s CPU cap | 1,204.611 |

For indexed 10,000-name import the independent parser probe took 73.549 ms,
and saving both stores took 21.212 ms. The benchmark counts a trailing newline,
so input byte sizes are one byte larger than the issue's strings.

Raw observations: [baseline](evidence/baseline.jsonl),
[indexed](evidence/indexed.jsonl).

Run the [bounded benchmark](../../../examples/named-import/README.md) to
regenerate the fixtures and compare both algorithms on your machine.

## Verification and remaining limits

The CLI regressions use 100/1,000/10,000 numeric and named fixtures with a
60-second per-child deadline suitable for debug CI. They verify text export,
repeat-import deduplication in another process, and binary archive restoration.
A small Unicode/punctuation/reserved-name graph also reads back through a
fresh process using the original scan path. Differential storage tests cover
raw updates, duplicate pairs, address reuse, uniqueness merges, cascading
deletes, name reassignment, and persisted reload.

Indexes add O(n) memory and are rebuilt once when enabled on an opened store.
They do not cache a stale name-to-address result. The per-call Unicode facade
construction remains, unconstrained queries still enumerate the map, and
save rewrites the files. Large inputs are still parsed in memory. The plain
scan algorithm remains slow for large named inputs; opt in for those workloads.
The CLI flag is Rust-specific; C# and browser behavior is unchanged.
