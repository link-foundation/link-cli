# IPC transports, version 1 (issue #140)

The LiNo text and binary protocols are byte streams. They need a reliable,
ordered, duplex channel whose two directions can each end (half-close) and
which can hang up. Every IPC mechanism below is normalized to such a channel,
so a `LinksServer`, a `LinksClient`, a relay or a user's own code can carry
either protocol over any of them. The mechanism is chosen by an **endpoint**
string, so a mechanism is a decorator layer under the protocol: optional,
interchangeable and stackable.

Rust (`link_cli::protocol::ipc`) and C# (`Foundation.Data.Doublets.Cli.Ipc`)
implement every kind on both the listening (server) and connecting (client)
side. The JavaScript client (`@link-foundation/link-cli-client/ipc`) connects
natively where Node.js has the needed system calls, and otherwise through a
`clink --serve stdio --forward ENDPOINT` bridge child process (see
[JavaScript](#javascript)). Any of them can talk to any other: the wire format
below is the contract.

## Endpoints

```text
endpoint = [kind ":"] address ["?" option *("&" option)]
option   = name "=" value
```

The kind is matched case-insensitively. An endpoint with no known kind prefix
is a TCP `host:port`, so every address accepted before this version keeps its
meaning. `exec:` takes no options: everything after `exec:` is the command.

| Kind | Address | Mechanism | Platforms |
| --- | --- | --- | --- |
| `tcp` | `host:port` | TCP/IP stream socket (loopback or not) | all |
| `udp` | `host:port` | UDP datagrams + the [reliable session](#reliable-datagram-session) | all |
| `unix` | socket path (`@name`: Linux abstract namespace) | Unix domain stream socket | Linux, macOS, Windows 10 1803+ |
| `pipe` | Windows: name or `\\.\pipe\name`; POSIX: directory | Windows named pipe (byte mode, duplex); on POSIX an alias of `fifo` | all |
| `fifo` | directory | two POSIX named pipes (FIFOs), one per direction | Linux, macOS |
| `stdio` | (empty) | the anonymous pipes of the process's standard input/output; listen only, one connection | all |
| `exec` | command | starts the command with anonymous pipes as its standard input/output; connect only | all |
| `shm` | directory | one shared memory object holding two [rings](#ring-region) | Linux, macOS, Windows |
| `mmap` | directory | one memory-mapped file holding two rings | all |
| `file` | directory | one regular file holding two rings, accessed with positioned reads/writes only | all |
| `signal` | directory | a memory-mapped ring file; each update is announced to the peer by an OS signal (POSIX) or a named event (Windows) | all |
| `mq` | directory | two POSIX message queues + the reliable session | Linux |
| `sysvmq` | directory | two System V message queues + the reliable session | Linux, macOS |
| `mailslot` | directory | two Windows mailslots + the reliable session | Windows |
| `copydata` | directory | two hidden message-only windows exchanging `WM_COPYDATA` + the reliable session | Windows |
| `alpc` | directory | two Windows ALPC ports exchanging datagrams + the reliable session | Windows (64-bit) |

A kind that the current platform lacks fails with an explicit "not supported
on this platform" error. No kind ever falls back to another mechanism.

`exec:` takes either a JSON array of strings (`exec:["clink","--serve","stdio"]`)
or words separated by whitespace (`exec:clink --serve stdio`); no shell is
involved.

### Options

Each side reads its own options. A value the two sides must share travels in
the handshake (`capacity`, `chunk`).

| Option | Kinds | Default | Meaning |
| --- | --- | --- | --- |
| `timeout` | all but `stdio` | `10000` | connect/handshake timeout, ms |
| `poll` | rings, rendezvous waits | `5` | longest sleep between two checks while idle, ms |
| `accept-poll` | rendezvous listeners | `20` | longest sleep between two scans of the directory, ms |
| `capacity` | `shm`, `mmap`, `file`, `signal` | `65536` | bytes of each ring (64..1073741824); chosen by the client |
| `chunk` | datagram kinds | `udp` 1200, `sysvmq` 1024, others 4096 | largest payload per datagram (16..65000); the smaller of both sides wins |
| `heartbeat` | `udp` | `1000` | an idle side sends `PING` this often, ms |
| `dead` | `udp` | `10000` | a peer silent this long is gone, ms |
| `rto` | datagram kinds | `20` | first retransmission timeout, ms (doubles up to 1000) |
| `signo` | `signal` (POSIX) | `SIGUSR2` | the signal this side wants to be woken by |
| `maxmsg` | `mq` | `8` | queue depth of each POSIX message queue (Linux allows 10 by default) |

Unknown option names are an error.

## Rendezvous directory

Kinds whose address is a directory (`fifo`, `shm`, `mmap`, `file`, `signal`,
`mq`, `sysvmq`, `mailslot`, `copydata`, `alpc`) meet in that directory. All
JSON below is one UTF-8 object with string and non-negative integer members;
unknown members are ignored. `ID` is 20 random lowercase hexadecimal digits,
fresh for every connection. Every file is written as `NAME.tmp` and then
renamed to `NAME`, so a reader never sees a partial file.

1. **Listen.** The listener creates the directory (and its parents; mode
   `0700` on POSIX). If it exists and holds `listener.json` of a living process,
   listening fails ("address in use"); a stale directory is cleaned of the files
   this protocol creates and reused. It writes `listener.json`:
   `{"v":1,"kind":"shm","pid":1234}`. `kind` is the canonical kind (`pipe`
   on POSIX is written as `fifo`).
2. **Request.** The client reads `listener.json`, checks `v`, `kind` and that
   `pid` is alive. It creates its part of the channel (per kind, below) and
   writes `ID.req`: `{"v":1,"kind":"shm","pid":5678, ...}`.
3. **Accept.** The listener scans for `*.req` (sleeping up to `accept-poll`
   between scans), reads and deletes the request, creates its part, and writes
   `ID.ack`: `{"v":1,"pid":1234, ...}`, or `{"v":1,"error":"text"}` to refuse.
4. **Connected.** The client reads and deletes `ID.ack`. A client that times
   out deletes `ID.req` and its resources.

On close the listener deletes `listener.json` and then the directory when it
is empty. A process is **alive** when `kill(pid, 0)` succeeds or fails with
`EPERM` (POSIX), or when `OpenProcess` succeeds and `GetExitCodeProcess` is
`STILL_ACTIVE` (Windows). While connected, a side whose peer process is no
longer alive treats the channel as hung up (checked at least every 100 ms of
waiting).

Per-kind members and resource names (`DIR` is the directory):

| Kind | Request members | Ack members | Resources |
| --- | --- | --- | --- |
| `fifo` | | | client creates FIFOs `DIR/ID.c2s` and `DIR/ID.s2c` (mode `0600`) |
| `shm` | | | client creates `/lcID` (POSIX `shm_open`, `0600`) or `Local\clink-ID` (Windows) |
| `mmap` | | | client creates `DIR/ID.mmap` |
| `file` | | | client creates `DIR/ID.ring` |
| `signal` | `"signal":N` on POSIX | `"signal":N` on POSIX | client creates `DIR/ID.sig`; Windows: client creates auto-reset events `Local\clink-ID-s` and `Local\clink-ID-c` |
| `mq` | `"chunk":N` | `"chunk":N` | client creates `/lcIDs` (server reads) and `/lcIDc` (client reads), `maxmsg` deep, message size `chunk + 8` |
| `sysvmq` | `"chunk":N,"server_queue":Q,"client_queue":Q` | `"chunk":N` | client creates two `IPC_PRIVATE` queues, mode `0600` |
| `mailslot` | `"chunk":N` | `"chunk":N` | client creates `\\.\mailslot\clink-ID-c` before the request; server creates `\\.\mailslot\clink-ID-s` before the ack |
| `copydata` | `"chunk":N,"hwnd":H` | `"chunk":N,"hwnd":H` | each side creates a message-only window (`HWND_MESSAGE` parent) on its own message-loop thread |
| `alpc` | `"chunk":N` | `"chunk":N` | client creates port `\RPC Control\clink-ID-c` before the request and accepts on it; server creates `\RPC Control\clink-ID-s`, connects to the client's port, then acks; the client then connects to the server's port |

On POSIX the client unlinks the names and files of `fifo`, `shm`, `mmap`,
`file`, `signal` and `mq` once it has read the ack and opened everything: both
sides keep them open, and nothing is left behind on a crash after that point.
On Windows each side deletes the files after closing them; whichever closes
last succeeds. System V queues are removed (`IPC_RMID`) by whichever side
closes first; the other side then sees the channel hang up.

### FIFO

The server opens `ID.s2c` for writing and `ID.c2s` for reading (both
non-blocking, then switched to blocking) before acking; the client opens
`ID.s2c` for reading before the request and `ID.c2s` for writing after the ack.
Each side first writes one byte `0x00` to its outgoing FIFO and discards the
first byte it reads: until that byte arrives, end-of-file only means the
writer has not opened yet. The rest is the raw byte stream; closing the
writing end is the end of that direction.

## Ring region

`shm`, `mmap`, `file` and `signal` share one region of `64 + 2 × (128 + C)`
bytes, created, zero-filled and initialized by the client before its request.
All integers are little-endian `u32`.

| Offset | Field |
| --- | --- |
| 0 | magic `LIPC` (`4C 49 50 43`) |
| 4 | version `1` |
| 8 | `C`, the capacity of each ring |
| 12..64 | zero |
| `R` + 0 | `head`: bytes ever written to this ring, modulo 2³² |
| `R` + 4 | `!head` (bitwise complement) |
| `R` + 8 | writer state: `0` open, `1` end of stream, `2` aborted |
| `R` + 64 | `tail`: bytes ever consumed, modulo 2³² |
| `R` + 68 | `!tail` |
| `R` + 72 | reader state: `0` open, `2` gone |
| `R` + 128 | `C` data bytes; stream byte `k` is at `k mod C` |

Ring 0 (`R = 64`) carries client→server bytes, ring 1 (`R = 192 + C`)
server→client bytes. The client initializes both complements to `0xFFFFFFFF`.

- **Counters.** A counter is valid only when its complement matches; a reader
  that sees a mismatch reads both again. A writer stores the value first and
  the complement second, with release ordering where memory is shared; readers
  load with acquire ordering. This lets the `file` kind and the JavaScript
  client, which use positioned reads and writes instead of atomics, read the
  region safely.
- **Write.** With `used = head − tail` (wrapping), copy up to `C − used` bytes at
  `head mod C` (wrapping), then publish `head`. Wait while the ring is full.
  Fail with "broken pipe" once the reader state is `2`.
- **Read.** Copy up to `used` bytes from `tail mod C`, then publish `tail`. When
  the ring is empty and the writer state is not `0`, load `head` and `tail`
  again: the writer may have published its last bytes between the first check
  and storing its state. Only when the ring is still empty has the direction
  ended.
- **Close.** Ending a direction stores writer state `1` after its last byte.
  Hanging up stores writer state `2` on the outgoing ring and reader state `2`
  on the incoming ring.
- **Waiting.** `shm`, `mmap` and `file` poll, backing off from busy checks to
  sleeps of at most `poll` ms. `signal` announces every publish of `head`,
  `tail` or a state: POSIX sends the peer's requested signal with
  `kill(peer_pid, signal)` (default `SIGUSR2`; Node.js reserves `SIGUSR1`), and
  Windows sets the peer's event (`Local\clink-ID-s` wakes the server,
  `Local\clink-ID-c` the client). A waiting side still re-checks at least every
  100 ms.

## Reliable datagram session

`udp`, `mq`, `sysvmq`, `mailslot`, `copydata` and `alpc` move frames. Each
frame is one datagram/message of an 8-byte header and a payload of at most
`chunk` bytes:

| Offset | Field |
| --- | --- |
| 0 | type: `1` DATA, `2` ACK, `3` EOF, `4` PING, `5` ABORT, `6` HELLO, `7` WELCOME |
| 1 | flags, `0` |
| 2..4 | `u16` zero |
| 4..8 | `u32` LE sequence number |

Each direction numbers its DATA and EOF frames from 1; the other types use 0
except ACK.

- **Sender.** Stop-and-wait: one unacknowledged DATA or EOF frame at a time.
  It is retransmitted after `rto` ms, then after doubling delays of at most
  1000 ms, until acknowledged or the peer is gone.
- **Receiver.** A frame whose sequence is the next expected one is accepted when
  at most 1 MiB is waiting to be read; it is then acknowledged with
  `ACK(seq)`. A smaller sequence (a retransmission) is acknowledged again with
  the last accepted sequence. Other frames are dropped unacknowledged.
- **EOF** ends the direction after the data before it. **ABORT** hangs up both
  directions at once and is not acknowledged. **PING** keeps a `udp` session
  alive and expects no reply.
- A `udp` peer is gone when nothing arrives for `dead` ms. Local kinds use the
  peer's process liveness.

Per mechanism:

- **`udp`.** The listener's socket only takes `HELLO` frames whose payload is
  a 16-byte random token followed by the client's `chunk` (`u32` LE). For each
  new `(source address, token)` the listener opens a socket on its own IP and
  an ephemeral port, connects it to the client and sends from it `WELCOME`,
  whose payload is the same token and the final `chunk`. A repeated `HELLO`
  gets the same `WELCOME` again. The client resends `HELLO` every 100 ms until
  `timeout`, then connects its socket to the `WELCOME`'s source.
- **`mq`.** One frame per message, priority 0; `mq_timedsend` and
  `mq_timedreceive`. A full queue counts as a dropped frame.
- **`sysvmq`.** One frame per message of `mtype` 1, sent with `IPC_NOWAIT` (a
  full queue counts as a dropped frame) and received with `IPC_NOWAIT` polling.
- **`mailslot`.** One frame per `WriteFile`; each side reads its own mailslot
  with a 100 ms read timeout.
- **`copydata`.** `SendMessageTimeoutW(peer, WM_COPYDATA, own hwnd, &cds,
  SMTO_ABORTIFHUNG | SMTO_BLOCK, timeout)` with `cds.dwData = 0x4C495031`
  (`LIP1`), `cds.cbData` the frame length and `cds.lpData` the frame. The
  window procedure copies the frame before returning: `1` when taken, `0` when
  dropped (for example on a full queue).
- **`alpc`.** Each frame is one `LPC_DATAGRAM` message (type 3) sent with
  `NtAlpcSendWaitReceivePort` on the port connected to the peer's inbox port;
  the inbox port owner receives with `NtAlpcSendWaitReceivePort` (100 ms
  timeout) and accepts the peer's single connection request
  (`LPC_CONNECTION_REQUEST`, type 10) with `NtAlpcAcceptConnectPort`.
  `LPC_PORT_CLOSED` (5) and `LPC_CLIENT_DIED` (6) hang up. Ports use
  `MaxMessageLength = chunk + 8 + sizeof(PORT_MESSAGE)`.

## Byte stream kinds

`tcp`, `unix`, `pipe` (Windows), `fifo`, `stdio` and `exec` carry the protocol
bytes unchanged; the native end-of-file is the end of a direction. A Windows
named pipe cannot half-close, so ending its outgoing direction closes it.

- **`unix`.** A listener removes a stale socket file (one nobody accepts on)
  before binding. Paths starting with `@` are Linux abstract names.
- **`pipe` on Windows.** `CreateNamedPipeW` with `PIPE_ACCESS_DUPLEX`,
  `FILE_FLAG_OVERLAPPED`, `PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT |
  PIPE_REJECT_REMOTE_CLIENTS` and unlimited instances; `FILE_FLAG_FIRST_PIPE_INSTANCE`
  on the first instance makes a second listener of the same name fail. Clients
  `CreateFileW` the pipe and wait with `WaitNamedPipeW` while it is busy.
- **`stdio`.** The listener serves exactly one connection on its standard
  input and output, then stops; its banner and logs go to standard error.
- **`exec`.** The connector starts the command without a shell, writes to its
  standard input and reads its standard output; standard error is inherited.
  Closing waits up to 5 s for the child to exit, then kills it.

## Stacking

Mechanisms stack in two ways, and both compose freely:

- **Several endpoints.** A server can listen on several endpoints at once
  (`clink --serve tcp:127.0.0.1:0 --serve shm:/tmp/a --serve mq:/tmp/b`). Every
  connection is a session of the same store; subscriptions push across
  mechanisms.
- **Relays.** `clink --serve A --forward B` accepts connections on `A` and
  forwards each one, byte for byte and in both directions with half-close, to
  a new connection to `B`. Chains such as `tcp → shm → udp → server` work in
  either protocol, because a relay never parses LiNo.

Both CLIs print `clink server listening on ENDPOINT` once per endpoint
(standard error for `stdio`; TCP keeps its historical `host:port` form) and
log each transport event to standard error with `--trace` or
`CLINK_IPC_TRACE=1`.

## Extension

Each implementation keeps a registry of kinds: `link_cli::protocol::ipc::register`,
`IpcTransports.Register` and `registerTransport` (JavaScript). A registered
kind receives the parsed endpoint and returns a duplex stream (connect) or a
listener (listen); built-in names cannot be replaced.

## JavaScript

The Node.js client connects natively to `tcp`, `udp`, `unix` (POSIX), `pipe`
(both meanings), `fifo`, `exec`, `file`, `mmap`, `signal` (POSIX) and `shm`
(Linux, through `/dev/shm`). It uses positioned reads/writes for ring regions,
which the complemented counters make safe, and `process.kill` / a `SIGUSR2`
listener for `signal`. Node.js has no API for POSIX or System V message
queues, mailslots, windows messages, ALPC, POSIX shared memory outside Linux,
Windows named events or Windows Unix sockets; for those kinds it starts
`clink --serve stdio --forward ENDPOINT` (the `clink` command is configurable)
and talks to it over anonymous pipes, which still selects exactly the
requested mechanism between the bridge and the server.
