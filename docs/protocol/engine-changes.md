# Incremental logical revision records

`RevisionEncoding::Changes` (Rust), `RevisionEncoding.Changes` (C#), and
`revisionEncoding: 'changes'` (JS/TS/WASM) select canonical `engine2` records.
The default `Snapshot` / `snapshot` still emits complete `engine1` records.
This setting is independent of state structure, isolation, validation, history,
constraints, admission and physical log decorators. Readers accept mixed streams
regardless of the encoding selected for following commits.

```text
engine2|REVISION|BASE|NEXT|OPERATIONS|CHECKSUM
```

`REVISION` and `BASE` are u64; `NEXT` is a positive u128 allocator high-water
mark. All numbers use ASCII decimal without signs, whitespace or leading zeros
(except the number zero). `BASE` must equal the preceding revision and
`REVISION` must equal `BASE + 1`, without overflow. The initial predecessor is
empty revision zero with allocator one. `NEXT` cannot decrease. It can exceed
the consumer's address width: opening an exhausted allocator is permitted,
while creating an out-of-range address fails. Existing addresses remain editable.

`OPERATIONS` is empty or a semicolon-separated list, sorted strictly by address:

| Operation | Meaning |
| --- | --- |
| `+ID,SOURCE,TARGET,NAME` | Insert or replace the complete row and optional name |
| `-ID` | Delete an existing row, including its name |

Addresses must fit the consumer's unsigned link type. `ID` is nonzero and less
than `NEXT`; sources and targets may be zero or external addresses unless a
selected constraint disallows them. `NAME` is lowercase hexadecimal of valid
UTF-8, with `-` for no name and an empty field for the empty name. This preserves
delimiters, Unicode, NUL and newlines without physical record separators.
Duplicate/unsorted IDs, unknown operations, missing/extra fields, unchanged puts
and absent tombstones are invalid. Encoders omit operations whose final row is
identical to the preceding row. An empty operation list is valid: accepted
identical-result writes and transient create/delete activity can still advance
the revision or allocator. A read-only transaction still emits no record.

`CHECKSUM` is exactly sixteen lowercase hexadecimal digits of FNV-1a-64 over
all UTF-8 bytes preceding its final separator. It detects accidental corruption,
not malicious modification. Validate checksum, grammar, progression, address
bounds and constraints before publishing a candidate. Invalid complete records
fail closed without rewriting the supplied journal. File adapters retain their
separate incomplete-final-line repair contract.

Recovery applies changes against the exact preceding state. Shared state clones
its immutable root and applies changed paths, without first decoding a full flat
map. Flat state retains its complete candidate-copy cost. Standalone branch
merge-base exports remain `engine1` images; a dependent `engine2` record cannot
serve as a saved base. Branch resume replays either format to attribute an
evicted base before accepting the branch.

The physical delta decorator passes `engine2` records through unchanged and
resets its complete-image compression chain. A following `engine1` record starts
a new chain; a physical `delta1` cannot refer to an `engine2` base. Mixed streams
retain exact logical records and contiguous revision checks. Older readers
reject the new mandatory tag; reopen with a compatible reader before selecting
this encoding. A complete `engine1` record inside a stream does not authorize
discarding its preceding history. There is no journal compaction or checkpoint
truncation API in this change.

[Independent vectors](engine-change-vectors.json) cover valid/mixed streams,
41 malformed u32 streams, physical delta composition and invalid compression
bases. They are consumed by Rust, C# and actual WASM, including the public JS
boundary. This is logical redo over immutable states; page WAL ordering,
doublewrite/torn-page repair, physical power-loss testing and LSM storage remain
separate unfinished requirements.
