# Finite boundary and recovery verification

Follow the read-set feature with measured native coverage gaps. These tests
exercise public failure semantics rather than treating test counts as coverage.

- [x] Extract uncovered source locations from the complete LLVM report; retain
  the report and a reusable triage script without changing coverage exclusions.
- [x] Verify ordered-journal recovery rejects gaps, duplicates and allocator
  regression even when every individual record has a valid checksum.
- [x] Verify saved branch descriptors must match both rows and allocator, not
  only a committed revision number.
- [x] Exercise finite allocator and revision exhaustion, diagnostic/log faults,
  current-revision snapshots, poisoned mutex extraction and closed read visibility.
- [x] Exercise default storage trait algorithms, callbacks, error propagation,
  matching filters and reference queries with a bounded in-memory fixture.
- [x] Add equivalent C# boundary cases and bounded network/header/object failures.
- [x] Run focused tests, full native suites, coverage and lint/format checks.
- [x] Publish one useful atomic step and inspect exact-SHA CI: `adb93d0`,
  37 successful, ten skipped and one neutral check.

Private Rust fixtures set a counter directly for the revision-overflow case;
no experiment loops to the counter limit. All records and stores are tiny.

## Reproduced diagnostic failure

On `e2e714f`, a Linux subprocess with stderr directed to `/dev/full` exits 101
after persisting its first engine revision. The same probe also exits 101 during
replica catch-up. Both failures come from `eprintln!` after storage work succeeds.
The regression runs two commits per mode and reopens the file journal; after
the change, both children succeed and each journal contains two rows. Engine
and replica traces now ignore diagnostic I/O errors while retaining opt-in
output and the original storage errors. C# already protects trace callbacks;
its existing throwing-callback regression remains part of the full suite.

The [Rust `eprintln!` contract](https://doc.rust-lang.org/std/macro.eprintln.html)
documents panics on diagnostic write failure. The replacement uses the fallible
`Write` implementation of the locked stderr handle and discards only that
diagnostic result; journal and replica errors retain their existing handling.

A read-only stderr handle is not an equivalent probe: Rust deliberately ignores
EBADF for stderr. The committed test uses Linux's immediate, bounded ENOSPC
device. It never fills a real disk or writes an unbounded stream. Other platforms
still run the portable storage, recovery, counter and C# callback cases.

Raw adapter contracts additionally verify file/pinned/named forwarding, external
changes through the names sidecar, retry after a sidecar flush failure and pinned
address validation. Persistent names metadata can outlive deleted data rows.
