# Constructor failure ownership audit

Durability adapters open a data mapping before the names mapping. Verify prompt
cleanup when the latter cannot open, while preserving ownership of borrowed data.

- [x] Reproduce a failed owned constructor retaining its data file handle.
- [x] Verify typed/named/simple constructors with one directory-blocked names path.
- [x] Ensure a failed borrowed constructor leaves the caller's store usable.
- [x] Release partially created names resources as well as owned data resources.
- [x] Verify immediate exclusive reopening and successful construction after repair.
- [x] Run the nine focused cases.
- [x] Run full C# coverage suites and formatting.
- [x] Publish and inspect exact-SHA CI.

No GC forcing or allocation stress. Each fixture opens at most two mapped stores;
FileShare.None tests owned-handle release without relying on process termination.
If Linux does not enforce .NET sharing, retain a Windows integration check instead
of claiming a Linux reproduction.

On baseline `6f9673b`, all three owned constructors fail the immediate
FileShare.None reopen assertion with an IOException reporting a live data
handle. The three original borrowed cases pass. The fix distinguishes ownership
while constructing and uses one guarded names initializer to release either the
mapping or its partially constructed facade chain. A fourth borrowed case covers
the public pinned constructor directly. Two finite memory faults target raw
facade initialization and later names-content initialization respectively.

All nine focused cases now pass. The quota-backed names test confirms its
exception originates in UnicodeStringStorage, after the raw facade exists, and
checks disposal before its own final cleanup. The unavailable-memory case
throws before native pointer access and verifies direct mapping cleanup.

All checks for `bd195a69c1247933ee31b43de3477635ff264dee` finish with 36
successes, 11 skips and one neutral CodeQL aggregate. Recent runs have matching
head SHA and creation time 2026-10-24T19:37:12Z, including the C# workflow
40126123807 and Rust workflow 40126123818. The platform tests pass, including
Windows; release-only jobs are skipped on this pull request.
