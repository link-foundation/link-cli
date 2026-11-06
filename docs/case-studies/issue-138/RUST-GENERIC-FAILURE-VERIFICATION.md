# Generic mapped growth failure

`PersistentFileMapped<T>` wraps upstream `platform-mem` 0.4.0 `FileMapped<T>`.
For values with destructors, a failed `grow` used to leave the owner unusable.
That is now fixed for quota and mapping-limit errors. Two windows that only
upstream can close remain, and they are listed below.

## The defect

Upstream [`grow`](https://github.com/linksplatform/mem-rs/blob/f3362cfe2abef9f99fd23c98776efe3bdadd5ff5/src/file_mapped.rs#L99)
unmaps the current view first, and only then runs `metadata`, `set_len` and
`mmap`, each of which can fail. If one does, the owner keeps a pointer to unmapped
memory, so the next safe read crashes. `shrink` has the same order.

Plain data already avoided this by preparing a second view (`grow_retained`). That
does not work for values with destructors, because two owners would drop the same
values twice.

Reproduced on the real 0.4.0 with a real file quota: growth returned OS error 27
and the next `allocated()[0]` exited with status 139 (SIGSEGV).

## The fix

`preflight_view` in `rust/src/storage/file_mem_grow.rs` runs before `grow` and
`shrink` retire anything. It extends the file and maps a throwaway byte view of the
new size while the current view is still live. Upstream's `set_len` then has nothing
to do, and its `mmap` repeats a call that has just succeeded. If preflight fails, the
error comes back with the view, the flush barrier and every value untouched.

Physical extension does not count as initialization: `initialized_bytes` only moves
after a successful growth, so a retry still fills the whole new tail.

Preflight needs the old and new views mapped at once, which plain-data growth
already required. Growth that only fits in the address space because the old view
goes first now returns an error instead of succeeding.

## Panicking tail destructors

Upstream `RawPlace::shrink_to` runs the tail destructors and only then lowers its
length. A destructor panic therefore left retired values reachable. Before the fix,
a finite test counted 6 drops of 4 values, with the second drop of a shared `Arc`
corrupting the heap. `shrink` now aborts the process if a tail destructor unwinds.
That is a hard stop, but this is the only sound option available from outside the
owner, and it applies only in a case that used to be undefined behavior.

## Verification

```sh
cargo test --test mapped_growth_admission_tests
CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo build --manifest-path experiments/issue-138/mapped-grow-failure/Cargo.toml
python3 experiments/issue-138/run-mapped-grow-failure.py --expect-retained
python3 experiments/issue-138/run-mapped-grow-failure.py --generic-repro
```

- `quota_rejected_destructor_growth_keeps_the_view_readable` runs in the normal test
  suite on Linux. It failed with status 139 before the fix.
- `panicking_tail_destructor_cannot_drop_a_value_twice` expects SIGABRT from the
  guard and fails if the owner's destructor is ever reached.
- `generic_preflight_failure_keeps_the_view_and_runs_no_destructor` covers both
  `grow` and `shrink` returning an error before anything is retired.
- The probe bounds its children: 256 MiB of address space, 8 MiB of stack, one page
  of file size, 5/6 seconds of CPU and 30 seconds of lifetime, with core dumps off.

## What still depends on upstream

- A failure that first appears between preflight and upstream's repeated steps, such
  as another process taking the last address space or a disk I/O error.
- A failed `set_len` during a partial shrink. Truncation cannot be tried in advance,
  and upstream has by then already retired the tail and unmapped the view.

Both leave the owner holding an unmapped pointer, and neither can be fixed from
outside, because the upstream fields are private. Both need upstream to map before it
unmaps, which is [reported upstream](https://github.com/linksplatform/mem-rs/issues/38).
