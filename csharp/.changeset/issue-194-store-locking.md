---
'Foundation.Data.Doublets.Cli': minor
---

Concurrent `clink` runs on one store no longer interleave their changes (#194).
A run now locks its stores for its whole read-modify-write: writers
exclusively, read-only runs of an existing store shared (readers creating a new
store at once crashed each other). A run that cannot get the lock within
`--lock-timeout <ms>` (default 30000) exits with a "store is locked" error that
names the owner. `--lock flock` is the default. It takes an OS lock on
`<db>.lock` (`flock` on Unix, `LockFileEx` on Windows), which the system
releases when the holder exits. `--lock pid` uses a `<db>.lock.d` directory
with an owner record (pid, host, PID namespace, start time), which a waiter
breaks once that owner process is gone. `--lock none` and `--no-lock` are for
callers that serialize access themselves. `--lock-status` prints who holds the
locks of a store. The Rust and JavaScript `clink` take the same locks, so mixed
runs exclude each other too. The library gains `StoreLock`,
`StoreLockOptions`, `StoreLockOwner`, `StoreFileLock` and `StorePidLock`.
