# Mirrored checkpoint roots

An optional `MirroredCheckpointRoot` decorates two exclusive raw page devices.
It stores no data pages. It decodes checkpoint decisions (`CLNKBK01`), separately
from the native page-bank mirroring protocol (`LNKDMT01`). Both devices remain
caller-owned in C#/JS; Rust extraction transfers ownership without recovery or
flush. A portable checkpoint log never closes the underlying devices.

The existing 128-byte decision layout is unchanged. The flags at offset 36
combine two independently selected bits:

| Root mode | Bank mode | Decision flags | Bank metadata flags |
| --- | --- | --- | --- |
| Raw | Raw | 0 | 0 |
| Raw | Mirrored pages | 1 | 1 |
| Mirrored checkpoint root | Raw | 2 | 0 |
| Mirrored checkpoint root | Mirrored pages | 3 | 1 |

The root adapter advertises flags 2 or 3, including its expected bank mode.
`CheckpointPagedLog` / `CheckpointPagedCommitLog` verifies that advertised mode
against the actual banks before any I/O or initialization. Root-mirrored files
require a provider supporting these flags. Older providers and accidentally
single-root writers reject them before mutation. Existing single-root files
retain their bytes and must continue to use their original mode. Creating a
second root is not an in-place upgrade: export into a separately owned store.
[Independent vectors](checkpoint-root-mirror-v1.json) include epochs beyond JS's
safe integer range and the maximum u64 epoch. JS represents epochs with bigint.

Each slot is read from both devices. Length, checksum, version, zero padding,
flags, bank parity and derived bank identity validate before selection. Two
valid copies must have the same database identity. Their epochs are equal or
differ by two (one interrupted publication to that slot); larger gaps reject.
Equal identity/epoch/flags uniquely determine canonical decision bytes. The
higher valid compatible epoch wins. Missing, torn or read-failed copies do not
outvote a valid copy. Two invalid nonempty copies fail closed. Unknown valid
versions or modes reject even if the other copy is readable. Across slots,
the usual matching-identity/adjacent-epoch rule still applies.

Selection only stages repairs. The checkpoint log opens and validates the
selected bank's entire committed prefix before completing root recovery.
Recovery rechecks every selected repair source before writing any destination,
then writes and synchronizes repaired slots. A changed source or any uncertain
recovery mutation fences the root adapter. Reopen all exclusive devices to
resolve the durable state. No read silently repairs a root before its bank
validates. Publication prepares and synchronizes the inactive bank first,
writes both root copies, then synchronizes both before acknowledgment.

This mode adds one root device, one extra root write and synchronization per
replacement, and two extra metadata reads at opening. Recovery rechecks and
synchronizes only damaged copies. Ordinary appends still write the selected
bank; they do not publish a new root epoch. Both root copies may still fail
together. Successful sync is assumed truthful; process exits and finite durable
images do not certify physical power-loss behavior.

Portable composition uses the same log API and separately owned bank devices.
For example, when `primary` and `secondary` are exclusive raw root devices and
`banks` contains two raw page devices:

```rust
let root = MirroredCheckpointRoot::new(primary, secondary, 0)?;
let mut journal = CheckpointPagedLog::open(root, banks)?;
journal.replace_log_entries(&["acknowledged".into()])?;
```

```csharp
var root = new MirroredCheckpointRoot(primary, secondary, bankFlags: 0);
var journal = new CheckpointPagedCommitLog(root, banks[0], banks[1]);
journal.Replace(new[] { "acknowledged" });
```

```js
import { MirroredCheckpointRoot } from '@link-foundation/link-cli-client/engine-checkpoint-root';
import { CheckpointPagedCommitLog } from '@link-foundation/link-cli-client/engine-checkpoint-pages';
const root = new MirroredCheckpointRoot(primary, secondary, 0);
const journal = new CheckpointPagedCommitLog(root, banks);
journal.replace(['acknowledged']);
```

For mirrored page banks, wrap each bank's two raw devices with
`MirroredPageDevice` and pass bank mode `1` to the root adapter. The checkpoint
log checks both bank modes. Acquire every raw device before constructing the
log; on closing, release banks before both roots. A raw root can still accompany
mirrored banks, and mirrored roots can accompany raw banks.

The [native owning factories](../ENGINE-NATIVE-CHECKPOINTS.md) select the same
composition with Rust `FileCheckpointOptions.mirrored_root`, C# `mirroredRoot`
or JS `mirroredRoot`. They acquire the primary root, optional `journal.mirror`,
both banks and optional bank mirrors before validation or repair. Each root
uses its own empty data file and `.meta0`, `.meta1`, `.writer` sidecars. The
original APIs still select a single root. The persistent handoff example
selects root mirroring with `--compact --mirrored-root`; bank mirroring remains
independently selected by `--mirrored`.
