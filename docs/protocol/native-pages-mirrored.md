# Optional mirrored native pages, version one

This extension of [the native page protocol](native-pages.md) selects immutable
metadata flags `1`. Single-copy metadata retains flags `0`. All other header,
doublet, UTF-8, count, checksum and canonical predecessor rules are unchanged.
Opening either format through a device with the other mode rejects without
mutation. Independent bytes and 36 finite damaged-copy instructions live in
`native-pages-mirrored-v1.json`, generated without production codec imports.

Two exclusive devices represent the same database identity and decisions. Native
files are `path` and `path + '.mirror'`, each with `.meta0`, `.meta1` and `.writer`.
Both atomic exclusive-create owners must be acquired; failed construction closes
all acquired handles. Uncertain close retains the relevant owner. Recovering a
crashed process requires external fencing before explicitly removing both owner
files. Neither provider steals a live or stale owner automatically.

A flush writes each new immutable page to both devices, then synchronizes both
data files. Only after both successful data syncs does it write the alternating
metadata decision to both devices and synchronize both decisions. Acknowledgement
follows both metadata syncs. Any mutation failure fences the device/log. Recovery
can discover the old or complete new group after an uncertain outcome; it never
publishes part of a group. Empty flushes perform no I/O.

For each metadata slot, recovery selects the highest checksummed supported
decision from its two copies. Missing/torn copies have no committed meaning;
checksummed unsupported formats, differing identities and conflicting bytes at
equal generation reject. The ordinary log then selects the newest decision
across alternating slots, applying the same conflict and portable-count rules.
Both unreadable copies of a nonempty slot reject rather than silently dropping
a potentially acknowledged decision.

Starting at that root, each data page must match the expected checksum from the
decision or next page. A locally valid stale checksum is insufficient. One exact
surviving page can supply a damaged, short or missing copy. Neither matching copy,
or two differing matching copies, rejects. The full selected prefix must pass
canonical doublet, identity, generation, count and strict UTF-8 validation before
any repair or tail truncation occurs.

Recovery tracks damaged indexes, expected hashes and source/destination sides.
After validation it rereads surviving pages, checks their expected hashes again,
and repairs in ascending page order. A missing suffix can be filled contiguously;
repair never creates holes. All repaired data is synchronized before redundant
metadata is repaired and synchronized. Subsequent recovery is idempotent. A
failed repair fences the owner and preserves the surviving source for reopen.

Selected data/metadata bytes, page writes and flush syncs double. The codec keeps
one page buffer; repair also tracks damaged indexes and two metadata decisions.
Open/read still scan history, and the log interface still returns all records.
The ordinary single-copy provider pays none of the additional copy I/O. Logical
revision encoding, state structure, validation, delta encoding and replica
acknowledgement remain independently selectable.

Successful sync must mean durable preceding writes; devices must preserve other
durable pages outside subsequent writes. FNV detects accidental damage, not
adversarial changes. Repair requires at least one surviving expected copy per
page and decision. Common-mode damage or loss of both copies is not generally
recoverable. Directory sync assumptions and Windows differences are inherited
from the base protocol. Finite fault tests and bounded process exits do not
certify real hardware power-loss behavior. This is a two-copy journal strategy,
not InnoDB doublewrite, consensus, a backup, or legacy mapped-file repair.
