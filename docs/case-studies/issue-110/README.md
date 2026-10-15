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

Linux x86_64, Rust 1.99.0, release build, one compiler job. Final measurements
use doublets 0.8.0, links-notation 0.26.0 and lino-arguments 0.5.0, inherited
from current main. Independent parser and import measurements have normal
scheduling variance. Milliseconds:

| Mode | Links | Parser (indexed run) | Import with scans | Import with indexes |
|---|---:|---:|---:|---:|
| Numeric | 100 | 0.620 | 0.539 | 0.658 |
| Numeric | 1,000 | 13.957 | 14.456 | 4.854 |
| Numeric | 10,000 | 63.694 | 1,425.246 | 75.116 |
| Named | 100 | 1.080 | 121.241 | 15.202 |
| Named | 1,000 | 7.836 | 11,211.748 | 121.363 |
| Named | 10,000 | 82.174 | exceeded 25s deadline | 1,272.974 |

For indexed 10,000-name import, saving both stores took 20.402 ms.
The benchmark counts a trailing newline,
so input byte sizes are one byte larger than the issue's strings.

Raw final observations: [current dependencies](evidence/current-dependencies.txt).
The initial [baseline](evidence/baseline.jsonl) and [indexed](evidence/indexed.jsonl)
observations used doublets 0.5.0 and links-notation 0.23.0, before merging main.

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
The optimization and CLI flag are Rust-specific.

## Dependency CI maintenance

The initial CI run failed the repository's latest-dependency gate on 18
declarations, including case-study manifests. Merging current `main` brings in
PR #120's updates and lockfiles. Rust now uses doublets 0.8.0,
links-notation 0.26.0 and lino-arguments 0.5.0; C# uses the matching notation release.

The notation update changes canonical text for empty references and control
characters to lossless `~1{hex}` literals. Legacy quoted input remains accepted;
compatibility and CRLF transport tests cover both ports. Older text peers need
a compatible parser for these literals.
The binary version 1 golden vectors remain unchanged. See the
[protocol notes](../../protocol/binary-links-notation.md).
