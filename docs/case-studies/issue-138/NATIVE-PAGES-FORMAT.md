# Candidate native doublets page protocol, version 1

This independently specified protocol is implemented by the investigation
prototype in `experiments/issue-138`, not yet a released journal provider.
It is a selected physical representation for the existing ordered record log;
logical engine revision, delta, isolation, history and replication policies are
separate. The prototype imports no SQLite provider.

All integers are unsigned little endian. The database identity is 16 opaque
random bytes. Strings are strict UTF-8, preserve BOM and NUL, and exclude CR/LF
as required by the ordered record interface. Hashes use FNV-1a-64, initial
`cbf29ce484222325`, prime `100000001b3`, reduced modulo 2^64 after each byte.
They detect accidental corruption; they provide no authentication.

## Data pages

Pages are 4,096 bytes, containing a 64-byte header and up to 252 doublets.
A row's address is `pageIndex * 252 + slot + 1`, with zero reserved as null.
Source and target are each 64 bits. Unused slots are zero and never reused after
publication; every nonempty flush begins new pages, including after a partial
page. This wastes up to 251 slots per flush rather than rewriting committed data.

| Byte offset | Length | Meaning |
| --- | --- | --- |
| 0 | 8 | ASCII `LNKDPG01` |
| 8 | 16 | Database identity |
| 24 | 8 | Zero-based physical page index |
| 32 | 8 | Generation that created this page, starting at one |
| 40 | 4 | Occupied cells, 1 through 252 |
| 44 | 4 | Reserved, zero |
| 48 | 8 | Previous physical page's hash, zero for page zero |
| 56 | 8 | Hash of bytes 0..55 followed by bytes 64..4095 |
| 64 | 4032 | Source/target cells followed by zero padding |

A record's UTF-8 bytes are split into chunks of at most seven bytes. Each chunk
is represented by a doublet whose source is
`(1 << 63) | (length << 56) | littleEndianPayload` and target is the next chunk
address or zero. Bits 59..62 and unused payload bytes are zero. A chunk with a
successor has length seven. Allocate chunks in reverse logical order, then a
record doublet `(firstChunkOrZero, previousRecordOrZero)`. Empty records have a
zero string root. Record and chunk references consume exactly the preceding
occupied cells, crossing page gaps as necessary; no orphan, forward, aliased or
cyclic cells are accepted. Thus the record root is the last occupied address.
Decoding walks backwards iteratively, then reverses the record sequence.

## Alternating metadata

The data path has two 128-byte sidecars, `.meta0` and `.meta1`. Generation zero
publishes the empty database in slot zero. Each nonempty flush increments the
generation and replaces slot `generation % 2`; an empty flush performs no I/O.

| Byte offset | Length | Meaning |
| --- | --- | --- |
| 0 | 8 | ASCII `LNKDMT01` |
| 8 | 4 | Version, one |
| 12 | 4 | Flags, zero |
| 16 | 16 | Database identity |
| 32 | 8 | Generation |
| 40 | 8 | Committed physical page count |
| 48 | 8 | Total occupied cells across committed pages |
| 56 | 8 | Ordered record count |
| 64 | 8 | Latest record root |
| 72 | 8 | Last committed page's hash |
| 80 | 40 | Reserved, zero |
| 120 | 8 | Hash of bytes 0..119 |

Every count/address must fit the common exact integer range. Page count is at
most `floor((2^53 - 1) / 4096)`; callers can select a smaller positive limit.
Empty state has all six fields zero. Nonempty state requires generation between
one and page count, occupied cells between page count and 252 times page count,
record count between generation and occupied cells, and a root in the last page.
Page generations are nondecreasing, start at one, end at the metadata generation,
and cannot skip a generation. Exact counts and canonical references are checked.

Recovery rejects checksummed unsupported versions/flags instead of treating them
as an empty database. A missing, short or checksum-invalid metadata copy is not a
valid decision. Valid copies must share identity; equal generations must have
identical bytes. Select the newest valid decision and validate its complete
committed page prefix. Committed page corruption rejects open without falling
back to an older root. Only after validation may the exclusive owner truncate
unpublished trailing bytes and synchronize that repair. No valid metadata with
nonempty data is an error, not permission to recreate it.

## Persistence and ownership

The parent directory must already exist. All language adapters acquire the same
`path + '.writer'` using atomic exclusive file creation before opening data or
metadata. Existing live or stale ownership fails immediately. Reclaiming a
crashed owner's file requires external fencing and explicit removal; age alone
is insufficient. Close releases owned handles before removing its ownership.
POSIX adapters synchronize the parent directory before publishing the initial
root. Windows file synchronization has separate filesystem/name-persistence
assumptions; it must not be described as POSIX directory synchronization.

Flush writes only new data pages, synchronizes data, writes the alternate
metadata and synchronizes that metadata, then acknowledges and publishes its
in-memory root. Any write/sync error fences the handle until reopening. Reopen
may discover either complete group after an uncertain outcome, never a partial
group. Previously acknowledged groups remain present under the persistence
model: successful synchronization is durable, and subsequent writes do not
modify committed data or both metadata copies. Torn unacknowledged metadata may
select the preceding group. Damage to an acknowledged metadata copy is a
separate corruption scenario; alternating roots alone cannot distinguish it
from a torn unacknowledged write or repair it. Independent redundant physical
repair is still required and must be selected explicitly.

Appending retains pending strings and UTF-8 bytes. Flush uses one page buffer
and writes O(new bytes/pages), independently of committed history length.
Open/read currently validate O(history) and the existing `read()` interface
returns all records; this does not solve incremental engine replay. The finite
image fault model and bounded real process exits establish specified test
schedules, not certification against every device's physical power-loss behavior.
