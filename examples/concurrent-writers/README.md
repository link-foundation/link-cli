# Concurrent writers on one store

`run.sh` starts 12 `clink` processes at once. Each one imports one link with
`--in` into the same database. The script then exports the store with
`--out` and checks that every writer that exited 0 finds its link there.
Arguments after the script name go to every `clink` run:

```bash
CLINK="node js/server/bin/clink.js" examples/concurrent-writers/run.sh             # --lock flock, the default
CLINK=rust/target/release/clink examples/concurrent-writers/run.sh --lock pid
CLINK=csharp/Foundation.Data.Doublets.Cli/bin/Release/net10.0/clink examples/concurrent-writers/run.sh
CLINK=clink WRITERS=32 examples/concurrent-writers/run.sh --no-lock                # may lose writes
```

With a lock, every implementation keeps every write:

```
writers=12 succeeded=12 links=12 lost_after_success=0
```

Without one (`--no-lock`, the behavior before issue #194) writers overwrite
each other's saves. The script then exits 1. Runs on Linux gave:

| `clink`    | `--no-lock` result                                       |
|------------|----------------------------------------------------------|
| JavaScript | `succeeded=12 links=8 lost_after_success=4`              |
| Rust       | `succeeded=12 links=3 lost_after_success=9`              |
| C#         | `succeeded=6 links=2 lost_after_success=4`, and 6 crashes |

Every run locks the database for its whole read-modify-write: exclusively
when it writes, shared when it only reads. A writer that cannot get the lock
within `--lock-timeout` (30000 ms by default) exits 1 and names the holder,
so no run exits 0 without its write. See
[the case study](../../docs/case-studies/issue-194/README.md) for the root
cause and the lock kinds.
