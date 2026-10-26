# Logical checkpoint journals

A checkpoint journal starts with exactly one `engine3` record:

```text
engine3|revision|next|rows|checksum
```

Its fields, row/name encoding and checksum follow the engine1 full-revision
format, with `engine3` included in the checksum input. The first revision can be
any unsigned 64-bit revision. Revision zero must have no rows and allocator one.
Every nonzero row address must be below the allocator, within the selected address
type and unique. Names retain the distinction between absent and empty, using
strict UTF-8. Selected row/name/reference constraints apply during replay.

Only the first record can be engine3. Later engine1 or engine2 records must advance
exactly one revision and never decrease the allocator. Changed-row records also
validate their exact predecessor. A second checkpoint, broken checksum, invalid
row, revision gap or backwards allocator rejects opening the journal. Independent
[vectors](engine-checkpoint-vectors.json) are shared by Rust, C# and actual WASM.

`Engine::export_checkpoint`, `LinksEngine.ExportCheckpoint` and
`WasmEngine.exportCheckpoint` return owned records for retained committed states.
The oldest retained state becomes engine3; later retained states become engine1
records with unchanged revisions. With history disabled only current is exported;
Last N exports its retained states; All exports every available revision,
including zero when it is retained.
Opening a checkpoint does not manufacture unavailable revision-zero history.

Exports retain each revision's committed allocator. This preserves exact saved
branch descriptors and committed holes, including created-then-deleted addresses,
while excluding pending reservations of the original live owner. Resuming a
branch requires its exact merge base to remain in the checkpoint journal; an
evicted base is rejected. Selected history retains one allocator value alongside
each snapshot; history-disabled owners retain only the existing current value. A copy is an independent database, not a
second coordinated writer: its future allocations can overlap private addresses
reserved by the original owner. Export leaves that owner's allocator, active
sessions, retained snapshots and live log unchanged. An uncertain owner rejects
export until it is reopened.

Export allocates snapshot strings proportional to retained row/name contents.
It does not rewrite local files/pages or provide atomic publication, flushing,
encryption or authentication. Callers own safe replacement and synchronization.
Old readers reject engine3: upgrade every reader/writer before enabling this
format, and retain an ordinary journal if rollback to an old reader is required.

The optional coordinated interval publishes exported records in the same atomic
generation comparison/update as ordinary commits. All selected shard generations,
including read-only dependencies, are still checked. Read-only groups publish no
rewrite. Journal lengths may shrink or stay equal after a real write; generations
remain the conflict token. A failed or lost response still requires a fresh read
to discover the outcome, with no automatic mutation replay.
