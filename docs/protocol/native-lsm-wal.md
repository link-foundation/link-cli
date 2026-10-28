# Native physical LSM WAL semantics, version 1

This selected component validates the logical WAL used by the physical LSM
composition. It performs no I/O and does not open, create, publish or retire
tables. A semantic replay result alone is insufficient to expose a database:
the physical store must validate every selected table, its binding below, the
combined retained rows and the complete provider namespace first. The physical
store composition is described in [native-lsm-store.md](native-lsm-store.md);
the native directory facade remains unfinished.

Rust selects `physical-lsm` and `engine::lsm::LsmWalState`; C# selects `LsmWal`;
JavaScript explicitly imports `@link-foundation/link-cli-client/engine-lsm-wal`.
The JS root and default native Rust engine do not load this component. SQLite
is optional and is not used by this protocol or codec.

The first record is a checkpoint with exactly six array elements:

```json
["native-lsm1","0","0","0",[],[]]
```

The counters are global modification generation, generation covered by selected
tables, and last durably reserved table ID. All counters, table IDs, digests,
manifest key counts and manifest byte lengths are canonical decimal u64 strings:
`0` or a nonzero digit followed by decimal digits, no leading zero, signs,
whitespace or other characters. The largest value is `18446744073709551615`.
Table IDs and row generations are positive. Native identities are exactly 32
lowercase hexadecimal characters. Records are well-formed Unicode JSON strings
without raw CR or LF; escaped line breaks in journal payloads are supported.
Equivalent JSON escapes and spaces are accepted. Unknown record formats fail.

Each manifest entry is `[tableId,nativeIdentity,digest,keyCount,storedBytes]`.
Entries have strictly increasing IDs no greater than the reservation counter.
The manifest is nonempty exactly when the covered generation is positive.
Stored bytes include the immutable table's 15-byte `native-sst1` marker, and
must be between 15 and 64 MiB. A descriptor may describe an empty sealed table;
the physical store must still establish the global generation from actual rows.
The selected immutable table's identity, digest, count and stored bytes must
match its descriptor; accepting matching metadata without reading the selected
file is insufficient. No payload digest here provides authentication.

Buffered rows are sorted unique `[key,generation,entriesOrNull]` triples, with
the same 1..128 ASCII alphanumeric/underscore/dot/hyphen key rules as immutable
tables. `null` retains a tombstone and its positive generation. `[]` is a live
empty journal. The covered generation is at most the global generation. Every
buffer generation is greater than covered and at most global. The largest
buffer generation, or covered if the buffer is empty, equals global. A reserved
but unselected ID is allowed even in an otherwise initial empty checkpoint.

Every subsequent record is exactly `["put",nextGeneration,rows]`. It contains
1..32 sorted unique changed keys, all at exactly the next global generation.
Generations advance exactly once per group without gaps or reuse. A group
replaces buffered rows with matching keys; retained buffer admission is checked
after each complete group. Conflicts, read-only operations, deletes of absent
keys and identical updates are not encoded as changed groups. Reservation and
manifest changes require atomic complete-WAL replacement with one checkpoint,
never an appended second checkpoint. A group after global u64 exhaustion fails.

| Limit | Default | Permitted maximum |
| --- | ---: | ---: |
| Total retained keys | 4096 | 65536 |
| Total logical UTF-8 bytes | 8 MiB | 8 MiB |
| Buffered keys | 64 | Total retained key limit |
| Buffered logical bytes | 1 MiB | Total logical byte limit |
| Selected runs | 4 | 64 |
| Complete WAL records | 1024 | 65536 |
| Complete encoded WAL UTF-8 bytes | 16 MiB | 64 MiB |

All selected limits are positive integers. Logical row cost is ASCII key bytes
plus eight generation bytes plus UTF-8 record bytes and one byte per record.
Tombstones and live empty rows both cost key bytes plus eight. Record count also
bounds containers for empty strings. Replay only sees buffered payloads; total
logical capacity must additionally be checked against selected table rows.
The supplied complete log is admitted before JSON parsing; a caller/provider
must impose its own bounds before allocating and returning that complete log.
No pre-allocation bound is promised for an arbitrary unbounded custom provider.

Recovery returns owned transient payload arrays. Encoding revalidates all public
state and selected limits, so mutating a returned replay result cannot publish
an invalid checkpoint through the encoder. Equivalent string escaping may
produce different encoded costs in each writer; each writer checks its actual
UTF-8 encoded size. A mutation-capable store must preflight these bytes before
calling its replacement log. This codec does not acknowledge a durable write.

`native-lsm-wal-v1.json` is generated by the independent, standard-library-only
`experiments/issue-138/make-lsm-wal-vectors.py`, which imports no production
codec. Its 111 finite cases include 11 accepted checkpoints/replay sequences,
malformed counters and manifests, generation and ID exhaustion, ordered group
replacement, tombstone/empty/Unicode payloads and admission boundaries. Typed
Rust/C# log contracts cannot supply the one non-string log-provider fixture;
JavaScript also rejects that shape. Actual physical publication and process-exit
testing remain required work in `PHYSICAL-LSM-PLAN.md`.
