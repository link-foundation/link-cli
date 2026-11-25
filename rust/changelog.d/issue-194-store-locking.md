---
bump: minor
---

Concurrent `clink` runs on one store no longer lose updates (#194). A run now
locks the store for its whole read-modify-write: writers exclusively, read-only
runs of an existing store shared. A run that cannot get the lock within `--lock-timeout <ms>`
(default 30000) exits with a "store is locked" error that names the owner.
`--lock flock` is the default. It takes an OS lock on `<db>.lock`, which the
system releases when the holder exits. `--lock pid` uses a `<db>.lock.d`
directory with an owner record (pid, host, PID namespace, start time), which a
waiter breaks once that owner process is gone. `--lock none` and `--no-lock`
are for callers that serialize access themselves. `--lock-status` prints who
holds the locks of a store. Saves now write a temporary file unique to the
process and created exclusively (`<db>.<pid>.<random>.saving`) instead of the
shared `<db>.saving`.
