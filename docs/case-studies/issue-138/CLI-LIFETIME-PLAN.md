# CLI-owned store lifetime audit

Continue R1/R8 through the compiled C# entry point, rather than relying on
process exit to release mapped storage.

- [x] Add a finite regression that calls the real entry point and immediately
      reopens each created store with exclusive sharing before any GC forcing.
- [x] Preserve the pre-fix result and identify the owning scope.
- [x] Release owned layers in reverse order, including constructor failures.
- [x] Verify raw cache, transactions, version operations and trigger modes.
- [x] Run complete Debug/Release coverage, formatting and release-fragment checks.
- [ ] Publish and verify exact-SHA CI.

The original five cases exercise normal cache return, malformed transactional
input, unknown version checkout, persistent triggers and a blocked journal
constructor. Each uses a private temporary directory and tiny inputs. Console
capture belongs to the existing nonparallel collection. Finalizer forcing is
only failure cleanup after the exclusive-open assertions.

All original five cases first fail with an IOException while requesting exclusive
access. The action owned its mapped data, names, journal, version and trigger
stores but only stopped the transaction worker after RunCli. Using declarations
now span construction and execution; wrappers stop before their borrowed stores
are disposed. Embedded triggers reuse the single base owner.

The expanded fixture passes ten ownership cases: cache, malformed transaction,
unknown checkout, separate triggers, blocked transaction/version/trigger stores,
async mutation, branch listing and embedded triggers. An independent 88-case
fixture covers every enumerable deconstruction arity with short/exact/long
single-pass input, bounded prefix consumption and disposal on enumeration error.
Four transport cases deterministically inject IOException/ObjectDisposedException
at write/flush, checking the protocol category, original exception and progress.
These 92 contract cases already pass; they are not presented as bug fixes.
