# Case study: concurrent `clink` writers lose updates (#194)

Issue: [#194](https://github.com/link-foundation/link-cli/issues/194). It was
found by [link-assistant/hive-mind#4143](https://github.com/link-assistant/hive-mind/issues/4143),
which keeps its bot state in a `clink` store and runs many `clink` processes on
it at once.

## The bug

Several `clink` processes writing one store at the same time lost updates, and
some of them failed. The reproduction from the issue starts 12 writers, each
importing one link with `--in`. It now lives in
[`examples/concurrent-writers/run.sh`](../../../examples/concurrent-writers/run.sh).
With the JavaScript `clink` of `main` (commit `c4c69f29`), three runs on Linux
gave:

```text
writers=12 succeeded=11 links=5 lost_after_success=6
writers=12 succeeded=9 links=6 lost_after_success=4
writers=12 succeeded=11 links=7 lost_after_success=4
      2 Error: Failed to replace database: /tmp/tmp.GjQMcXMMS4/s.links
      1 Error: Failed to replace database: /tmp/tmp.GjQMcXMMS4/s.names.links
```

The issue reports the same with the Rust `clink` 3.1.2: 8 writers exited 0 and
only 2 links were left. The C# `clink` had the same race and failed worse:

```text
writers=12 succeeded=0 links=0 lost_after_success=0
     10 Unhandled exception: Foundation.Data.Doublets.Cli.UnexpectedLinkStructureException: Unexpected link found at address 1. Expected: (1: 1->1), Found: (1: 0->0).
      1 Unhandled exception: System.Collections.Generic.KeyNotFoundException: Link 2 does not exist.
```

In another C# run, 6 writers exited 0 and 4 of their links were missing.

## Root cause

Two independent defects, the first in all three implementations:

1. **No lock around the read-modify-write.** A run opens the store, reads it
   into memory (Rust, JavaScript) or maps it (C#), applies the query, and saves
   it. Nothing kept another run from doing the same in between. The Rust and
   JavaScript save replaces the whole file, so the last writer wins: it writes
   back the copy it read before the other writers saved, and their links are
   gone although they exited 0. C# maps the file and changes it in place, so
   concurrent writers see each other's half-written links and crash.
   `FileLock` (Rust `storage::lock`, C# `LinksFileLock`, JavaScript
   `storage-lock.js`) existed for library callers since #98, but the command
   line never took it.
2. **One temporary file for every writer.** Rust and JavaScript saved through
   a fixed `<db>.saving` (`saving_path` in `rust/src/link_storage.rs`) and
   renamed it over the database. Two writers wrote into the same file, and the
   one that renamed second found it gone: `Failed to replace database ... No
   such file or directory`.

## The fix

Each defect is fixed in all three implementations, which share one design (see
[ARCHITECTURE.md](../../ARCHITECTURE.md#store-locking)):

- **The whole run holds a store lock.** Before it opens anything, a run locks
  the database and each side store it will open, in path order. It holds the
  locks until the stores are saved and closed: exclusively when it writes,
  shared when it only reads (`--out`, a bare `--after`). `--serve` holds the
  lock until the server stops.
- **Lock kinds**, as the issue suggested:
  - `--lock flock` (default): an OS lock on `<db>.lock`, released by the OS
    when the holder exits. The exclusive holder writes an owner record to
    `<db>.lock.owner`, so a waiter can name it.
  - `--lock pid`: the directory `<db>.lock.d`, built under a unique name with
    its `owner` record (pid, host, PID namespace, process start time) and then
    renamed into place, so exactly one process publishes it. A waiter breaks
    it when the owner is on this host and in this PID namespace and its process
    is gone, or its PID now belongs to a process with another start time. This
    is the lock hive-mind used as its workaround (`src/state-lock.lib.mjs`).
  - `--lock none` and `--no-lock`: no lock.
- **`--lock-timeout <ms>`** (default 30000) bounds the wait. When it runs out,
  the run exits 1 with `Lock error: store is locked: <path> is held by
  <holder>; gave up waiting for the <kind> lock after <ms> ms
  (--lock-timeout)`. No run exits 0 without its write.
- **`--lock-status`** prints the state of every lock of the store and its
  owner, and `--trace` reports each wait (`held by ...`), each acquisition and
  each broken stale lock.
- **A temporary file per save.** Rust and JavaScript write
  `<db>.<pid>.<random>.saving`, created exclusively (`O_EXCL`), and rename it
  over the database, so two writers never share one even with `--no-lock`.

The three implementations write the same owner record and messages, so a C#,
a Rust and a JavaScript `clink` on one store exclude each other and name each
other.

## Requirements and evidence

| Requirement (issue #194) | Implementation | Verified by |
|---|---|---|
| Lock `<db>.lock` for the whole read-modify-write, exclusive for writes, shared for `--out` and reads | `rust/src/store_lock/`, `rust/src/main.rs`; `js/server/src/store-lock.js`, `src/main.js`; C# `StoreLock*.cs`, `CliStoreLocks.cs`, `Program.cs` | `rust/tests/cli_store_lock_tests.rs`, `js/server/test/main-store-lock.test.mjs`, `CliStoreLockTests.AWriterWaitsForAReaderToFinish` |
| `--lock-timeout <ms>`, `--no-lock` | the CLI parsers of all three | tests of the options in the same files |
| A temporary file unique per process, created exclusively | `rust/src/link_storage.rs`, `js/server/src/link-storage.js` | `js/server/test/link-storage.test.mjs` and the Rust storage tests |
| N concurrent `--in` imports, every successful exit present in `--out` | regression tests | 8 to 12 writers under each lock kind in all three suites, Rust and JavaScript writers mixed in `js/server/test/differential/store-lock.test.mjs`, and `examples/concurrent-writers` in the JS server workflow |
| `--lock flock` (default), `pid`, `none` | `store_lock` in each implementation | unit tests of each kind: shared readers, the owner record, stale `pid` locks broken, live ones kept |
| A `pid` lock is broken when its owner on the same host is gone | `pid_lock.rs`, `StorePidLock.cs`, `store-lock.js` | tests with a gone PID and with a reused PID (other start time) |
| On timeout, exit non-zero naming the owner | `LockedError` / `StoreLockedException` | each suite asserts the message; the differential test asserts that Rust names a JavaScript holder and back |
| `--lock-status`, or verbose output naming the holder | `--lock-status`, `--trace` | status and trace tests in all three suites |

After the fix, `examples/concurrent-writers/run.sh` gives the same line for
every implementation and lock kind on Linux:

```text
writers=12 succeeded=12 links=12 lost_after_success=0
```

and the old behavior comes back only with `--no-lock`:

| `clink`    | `--no-lock` result                                      |
|------------|---------------------------------------------------------|
| JavaScript | `succeeded=12 links=8 lost_after_success=4`             |
| Rust       | `succeeded=12 links=3 lost_after_success=9`             |
| C#         | `succeeded=6 links=2 lost_after_success=4`, 6 crashes   |

With `--no-lock`, Rust and JavaScript no longer fail with `Failed to replace
database`, since every save has its own temporary file. They still lose updates,
because nothing serializes the read-modify-write.

## Limits and trade-offs

- **All processes on a store must use the same lock kind.** A `flock` and a
  `pid` lock do not see each other. On Node.js before 26 the JavaScript
  `clink` cannot take a `flock` (it needs `node:ffi`), so it takes the `pid`
  lock by default. There, the other implementations need `--lock pid` too.
- **`--no-lock` loses updates by design.** It is for callers that serialize
  access themselves, as hive-mind did.
- **`flock` on network file systems.** The OS lock is only as good as the file
  system's: some NFS setups and bind mounts across containers do not share it.
  `--lock pid` exists for those.
- **Breaking a `pid` lock is checked, not atomic.** The waiter renames the stale
  directory to a tombstone and checks that it holds the record it read. If
  another process published a new lock in between, the directory goes back,
  but in that window a third waiter may see no lock. This is the usual
  limitation of directory locks, hive-mind's included.
- **A `pid` lock cannot be judged across hosts or PID namespaces.** A lock held
  from another host or container is never broken automatically. It has to be
  removed by hand after checking `--lock-status`. A `<db>.lock.d` without a
  readable record is treated as abandoned after 10 seconds.
- **Leftovers after a crash.** `flock` itself is released by the OS, but a
  crashed writer can leave `<db>.lock.owner` (removed by the next acquirer) and
  a `<db>.<pid>.<random>.saving` file (never read, safe to delete).
- **New files.** Every locked run leaves an empty `<db>.lock` next to the
  store, which a store with `--lock flock` needs anyway.
- **C# on Unix** maps `FileShare` onto `flock`, so
  `DOTNET_SYSTEM_IO_DISABLEFILELOCKING=1` turns its `flock` lock off.
- **A store reached through a symbolic link to the file itself** is locked as
  `<link>.lock`, next to the link, not next to its target: only the directory
  part of a path is resolved. Processes that name the store by the link and by
  the target do not exclude each other, so all of them should use one name.
  Symbolic links to directories are resolved, as are `..` and relative paths.
- **C# cannot open a Rust or JavaScript store file.** This is an existing
  format difference, not part of this issue, so mixed C# and Rust writers were
  verified to exclude each other but not to share data.
