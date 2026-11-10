# Optional IPC layers for clink

`link-cli-ipc` decorates the existing Rust/C#/JS clients and servers with native
IPC. Python 3.13 or later is required only for this optional layer. Install with
`python -m pip install ./ipc`, then use `clink-ipc`; from a checkout, use
`PYTHONPATH=ipc python -m link_ipc` on POSIX or set `PYTHONPATH=ipc` on Windows.
The package has no runtime dependencies and does not build either database.

```text
Rust / C# / JS client
  -> TCP client adapter -> selected IPC -> server adapter -> TCP clink --serve
```

Both adapters use the same command. Each incoming connection gets its own
outgoing connection and therefore its own database session. They copy bytes in
both directions, including subscription pushes, without parsing LiNo. Text,
binary, all binary options, authentication, permission filtering, doublets and
triplets keep their existing wire behavior. You can omit either adapter when
an application uses the Python stream API directly.

## Configure either end

Start the existing database server, then a server adapter and a client adapter:

```sh
clink --db db.links --serve 127.0.0.1:7777
clink-ipc --listen shm:/tmp/clink-memory --connect tcp:127.0.0.1:7777
clink-ipc --listen tcp:127.0.0.1:7778 --connect shm:/tmp/clink-memory
clink --connect 127.0.0.1:7778 --protocol binary '(create: (0 0))'
```

On Windows, use a fresh absolute directory such as `shm:C:\Temp\clink-memory`
at both ends. Choose `pipe:my-database` for a native Windows named pipe, or
`copydata:C:\Temp\clink-copy` for paired WM_COPYDATA windows. POSIX `pipe:`
selects paired FIFOs; `fifo:` names that mechanism explicitly.
Windows pipe identifiers are independent of each process's working directory;
use the same `pipe:` address at both ends.

| Endpoint | Payload mechanism | Platforms |
| --- | --- | --- |
| `tcp:127.0.0.1:7777`, `tcp:[::1]:7777` | TCP byte stream | Linux, macOS, Windows |
| `udp:127.0.0.1:7777`, `udp:[::1]:7777` | Loopback UDP with ordered delivery, acknowledgments and deduplication | Linux, macOS, Windows |
| `unix:/tmp/clink.sock` | Unix domain stream socket | POSIX |
| `shm:/tmp/clink-memory` | Native shared memory, two locked mailboxes | Linux, macOS, Windows |
| `mmap:/tmp/clink-map` | Two memory-mapped backing files | Linux, macOS, Windows |
| `file:/tmp/clink-disk` | Two bounded disk-file mailboxes | Linux, macOS, Windows |
| `signal:/tmp/clink-signal` | Shared memory payload, SIGUSR1 notifications and a wakeup pipe | POSIX |
| `fifo:/tmp/clink-fifo` | Two native named FIFOs | POSIX |
| `pipe:name` | Native duplex Windows named pipe; POSIX uses FIFOs at the supplied path | Windows, POSIX |
| `mq:/tmp/clink-queue` | Two native POSIX message queues | Linux with POSIX MQ support |
| `alpc:C:\Temp\clink-alpc` | Two native ALPC ports | 64-bit Windows |
| `copydata:C:\Temp\clink-copy` | Paired hidden windows, WM_COPYDATA | Windows desktop/session |
| `mailslot:C:\Temp\clink-mail` | Paired **local** mailslots with ordered delivery and acknowledgments | Windows |
| `stdio:` (listen only) | Inherited anonymous stdin/stdout pipes | Linux, macOS, Windows |
| `exec:["program","arg"]` (connect only) | Anonymous pipes to a child started without a shell | Linux, macOS, Windows |

`tcp:` and `udp:` listeners accept port `0`; the actual address is printed to
stderr. `--ready-file PATH` writes `{"endpoint":"kind:address"}` to a new file
for orchestration. Status and opt-in `--trace` byte counts always go to stderr;
stdout remains binary clean for anonymous pipes.

Local native listeners own a **new** directory. Existing directories are
rejected, preserving existing data. Each accepted connection allocates unique
resources in its own session directory. A local Unix socket (POSIX) or named
pipe (Windows) exchanges bounded JSON configuration and monitors peer lifetime;
**payload uses the mechanism in the table**, never that control pipe. Normal
shutdown removes owned resources. After an uncatchable kill, remove the stale
endpoint directory only after verifying its owner has stopped; do not point a
new listener at an existing directory. Shared-memory clients also unlink their
session's blocks on close, including when the allocating peer has crashed.

## Stack layers

Each adapter connects to another endpoint, so a stack can use different
mechanisms in sequence:

```sh
clink-ipc --listen shm:/tmp/final-hop --connect tcp:127.0.0.1:7777
clink-ipc --listen mmap:/tmp/middle-hop --connect shm:/tmp/final-hop
clink-ipc --listen tcp:127.0.0.1:7778 --connect mmap:/tmp/middle-hop
```

Use `stdio:` with `exec:` to put anonymous pipes anywhere in the chain. The
`exec:` address is a JSON argv array; it is never evaluated by a shell. The
adapter terminates and reaps only the child it starts. See
[examples/ipc](../examples/ipc/README.md) for a runnable stack.

## Limits and lifecycle

- Each direction copies at most 4096 bytes per step; partial reads/writes,
  embedded zeroes and boundaries between text/binary packets are preserved.
  Shared-memory/file mailboxes hold one chunk per direction; POSIX queues hold
  four. WM_COPYDATA has one receive slot. UDP and local mailslots use one
  receive slot and stop-and-wait sequence/ACK framing. Native socket/pipe/ALPC
  buffers are also bounded by the OS.
- `--max-connections` (default 16, maximum 256) bounds relay workers. A session
  has separate read/write paths, so unsolicited pushes do not wait for another
  request. EOF is directional: buffered bytes precede EOF, and replies can
  still arrive after the request side closes.
- `--timeout` (default 30 seconds) limits connection setup and stalled writes
  where supported by the native API. UDP/mailslots also use heartbeats to
  detect a silent dead peer. Other idle connected sessions have no idle
  deadline. `--poll` (default 0.01 second) controls mailbox polling; signal
  wakeups reduce latency while polling remains a safeguard against coalescing.
- A failed session is closed. Requests are **never** reconnected or replayed.
  A failure after sending bytes can have an unknown database outcome. Check
  the application's operation result before issuing another mutation.
- `SIGINT` / `SIGTERM` stop listeners and active relays; `--once` exits after
  one connection. `stdio:` always accepts just one connection.
- `signal:` explicitly reserves process-wide SIGUSR1 and Python's signal
  wakeup FD. Embed it only in a process that owns these facilities. Conflicting
  SIGUSR1 handlers are rejected. Signals alone cannot carry arbitrary LiNo;
  the shared-memory payload is an intentional stack component.
- POSIX MQ may be unavailable due to kernel configuration, quotas or a
  container policy. Native failures are surfaced rather than falling back to
  another transport. ALPC uses the optional Windows native ABI; unsupported
  exports or layouts fail explicitly. WM_COPYDATA requires compatible Windows
  sessions/integrity levels. Mailslots use only `\\.\mailslot\...`; the
  deprecated remote mailslot protocol is never enabled or required.

These are local IPC adapters, with POSIX private directories, mode-0600
sockets/shared-memory/queue resources, and Windows native object permissions
inherited from the process. Mailbox files are protected by their private parent.
Use a private endpoint parent directory/appropriate Windows ACLs. They do not
add encryption or bypass the database's authentication/permissions. Bind the
existing TCP boundary to loopback. `udp:` deliberately accepts only numeric
loopback addresses; `tcp:` can also be used for explicitly configured networks.

## Embedding and additional mechanisms

```python
from link_ipc import Options, connect, listen, relay

listener = listen("shm:/tmp/application", Options(timeout=10))
try:
    with listener.accept() as incoming, connect("tcp:127.0.0.1:7777") as outgoing:
        relay(incoming, outgoing)
finally:
    listener.close()
```

`Stream` requires `recv(size)`, `sendall(bytes)`, `shutdown_write()` and
`close()`, with one concurrent reader and writer. `close()` must unblock I/O.
Listeners expose `address`, `accept()` and `close()`. Register another
mechanism with `register("custom", connector=factory, listener=factory)`;
factories receive `(address, Options)`. Existing names cannot be overwritten.
Adapters can wrap inherited descriptors, an existing RPC channel or a message
bus without changing either LiNo protocol. Notification-only primitives
(semaphores, eventfd, futexes, file notifications) belong alongside a payload
channel, as demonstrated by `signal:`. This explicit extension contract keeps
the open-ended “and all other similar” requirement extensible.

## Verification

```sh
PYTHONPATH=ipc python -m unittest discover -s ipc/tests -v
```

The native suite uses independent processes and verifies binary-safe bytes,
partial reads, EOF, resource cleanup, UDP loss/duplicate handling, crash
detection and composition. The IPC workflow runs it on Linux, macOS and
Windows, and runs real text/binary clink clients against Rust and C# servers,
including native Windows endpoints, subscription pushes and mixed stacks.
Supply `CLINK_COMMAND` as a JSON argv array to run those integration tests
against an already-built CLI locally; the tests do not trigger a source build.
