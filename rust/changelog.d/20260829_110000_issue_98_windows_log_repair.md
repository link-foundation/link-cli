---
bump: patch
---

Fixed opening a `transactions::FileTransitionLog` whose tail was left
half-written by a crash on Windows (issue #98). The torn tail is trimmed
with `File::set_len`, but Windows grants an append-only handle
`FILE_APPEND_DATA` without `FILE_WRITE_DATA`, so the call failed with
`ERROR_ACCESS_DENIED` and the log could not be reopened at all. The
repair now runs through a dedicated read/write handle, and the
append-only handle is opened afterwards.

Also replaced a `chunks_exact(2)` loop in the balanced variant converter
with `as_chunks::<2>()`, which the Rust 1.98 `clippy::chunks_exact_to_as_chunks`
lint rejects under `-D warnings`.
