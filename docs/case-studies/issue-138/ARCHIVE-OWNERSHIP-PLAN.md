# Exclusive ownership of legacy chunk archives

R1/R4/R7/R8 require retention not to destroy an older writer's archive. A
finite public-API probe uses two independent journals, a shared archive directory,
a frozen wall clock and fresh process counters. Both commits acknowledge, but
only one archive survives; the second writer replaces the first file's bytes.

- [x] Reproduce through public APIs with bounded child memory, stack and deadlines.
- [x] Run automated file, symlink and directory collision regressions before
  changing production code, and preserve actual failing behavioral logs.
- [x] Claim archive paths atomically, retry collisions without truncating or
  following existing entries, and preserve ordinary filesystem error semantics.
- [x] Review C# parity, recovery, repeated collisions, following commits, retained
  journal authority, and optional default-off diagnostic tracing.
- [x] Run applicable full tests, coverage, lint, release and source-size checks.
- [x] Run both CLI examples with independent databases sharing archive storage.
- [x] Commit/push only the prepared branch, update PR 139 and verify exact-head CI.

Rust's [OpenOptions::create_new](https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.create_new)
atomically creates a previously absent path and refuses existing symlinks.
The check must happen in the open operation, rather than an earlier existence
check followed by a truncating create. Retry only an actual collision; report
other I/O errors. A candidate counter advances on retries. This prevents
filename reuse across independent journals without imposing global transaction
IDs, a full directory scan or new work on bare stores.

C# previously used a random GUID with truncating StreamWriter semantics.
Its corresponding ownership contract now uses
[FileMode.CreateNew](https://learn.microsoft.com/en-us/dotnet/api/system.io.filemode?view=net-10.0).
Do not infer that a timestamp/counter repeat reproduces a GUID collision.

The live journal retains authoritative records. Exclusive archive ownership
alone does not promise atomic publication of complete archive contents after
write faults, power-loss durability, idempotent retention after recovery, or
native filesystem capabilities in WASM. Verify and report those limits.

Four Linux native contracts first fail: file bytes replaced, symlink target bytes
replaced, dangling symlink target created, and an occupied directory failing the
commit instead of retrying a candidate. All four now preserve existing entries
across repeated collisions, following commits and repeated recovery. Every child
has a ten-second deadline, 512 MiB address-space limit and 8 MiB stack limit.
A separate native control verifies an unrelated archive error retains the known
commit and fences until cleanup retry; two reopenings preserve subsequent edits.
The original public-API probe now retains both complete archive files.

The C# writer was extracted without changing its truncating behavior, then its
physical filesystem contracts first fail for an existing file, a symlink and a
dangling symlink. The existing-directory case is a passing control. Exclusive
creation makes all four pass. Two bounded sync/async controls verify optional
retention errors keep history for later retry and two reopenings. Eight competing
writers verify exactly one complete owner of the same candidate. This tests the
occupied-path writer contract, not a claimed reproduction of random GUID reuse.
C# retains its optional archive-error behavior; a later retention attempt gets a
new GUID rather than retrying the same supplied path. Symlink contracts execute
on Linux; Windows runner privileges are not assumed.

Full local validation passes: 742 native Rust, 408 actual WASM, 1,580 C# per
configuration and 106 JS, with strict lint, formatting, types, source-size and
release checks. Both CLI examples pass their shared-archive byte comparisons.
See archive-ownership.txt for exact coverage and evidence. All nine workflows
pass on published head 7b03c07; see archive-ownership-ci.txt. Later changes need
fresh exact-head CI.
