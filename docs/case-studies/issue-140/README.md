# Issue 140: every IPC mechanism as decorators under both LiNo protocols

The LiNo text and binary protocols are byte streams. [IPC transports
v1](../../protocol/ipc-transports-v1.md) normalizes every requested IPC
mechanism to one reliable, ordered, duplex byte stream that can half-close and
hang up. The Rust (`link_cli::protocol::ipc`) and C#
(`Foundation.Data.Doublets.Cli.Ipc`) libraries implement every kind on the
listening and the connecting side. The JavaScript client
(`@link-foundation/link-cli-client/ipc`) connects to every kind too. An
endpoint string picks the mechanism, so a mechanism is a decorator layer under
the protocol: optional, interchangeable and stackable on the client, the
server and a relay. Rust, C# and JavaScript interoperate over every kind, and
CI runs the check on Linux, macOS and Windows.

## Collected evidence and scope

The [original issue](https://github.com/link-foundation/link-cli/issues/140)
lists thirteen mechanisms and asks for "all other similar" ones. It also asks
for both the text and binary protocols, optional, interchangeable and
stackable decorators on both ends, and this case study, all in one PR. The
issue as filed is preserved in [`evidence/issue.json`](evidence/issue.json).
It had no comments, reviews, images or required sub-issues; the other JSON
files keep those empty collections and the first state of the PR.

The first iteration of PR #175 added a separate Python relay package (`ipc/`)
in front of the TCP boundary of the existing servers and clients. The owner's
review ([`evidence/pr-feedback.json`](evidence/pr-feedback.json)) asked for
something else: "implemented as decorators in C# + Rust and also if needed to
be used in JavaScript and clients in other languages. So we have full
implementation in code of all possible methods of inter process
communication." The scope is the entire repository, nothing is deferred, and
every change is applied everywhere it belongs.

The final design follows that review:

- **Rust and C# implement every mechanism themselves.** They do not relay
  through another runtime.
- **JavaScript connects natively wherever Node.js can.** For the other kinds it
  starts the `clink` bridge.
- **The Python package is removed.** Its relay is replaced by `clink --serve A
  --forward B` in both CLIs, and its private wire formats are replaced by one
  written specification that all three implementations follow.

[PLAN.md](PLAN.md) maps the work to its verification.

The protocol code before this issue was bound to TCP:

- **Rust:** `protocol/server.rs` accepted `TcpStream`s, and `LinoConnection`
  owned one.
- **C#:** `LinksServer` and `LinoConnection` used `TcpListener` and `Socket`.
- **JavaScript:** `connectTcp` used `node:net`.

The protocol framing never needed TCP. So the change replaces the socket with
an `IpcStream`/`IIpcListener` (Rust: `IpcStream`, `Listener`) everywhere. The
sessions, authentication, subscriptions, triplets and both wire formats of
[PR #160](https://github.com/link-foundation/link-cli/pull/160) and
[PR #155](https://github.com/link-foundation/link-cli/pull/155) are unchanged.
A bare `host:port` is still TCP, so every existing command, test and example
keeps its meaning.

Some kinds exist only on some platforms: ALPC, `WM_COPYDATA` and mailslots
only on Windows, and POSIX message queues only on Linux. A kind the platform
lacks fails with "not supported on this platform", and no kind ever falls back
to another mechanism. "All other similar" mechanisms cannot be listed ahead of
time, so each implementation has a registry (`register`, `IpcTransports.Register`,
`registerTransport`) for new kinds. `sysvmq` (System V message queues) is
added as the closest similar mechanism, and it covers macOS, which has no
POSIX message queues.

## Requirements, solutions and verification map

| Requirement | Kind and mechanism | Verification |
| --- | --- | --- |
| Shared memory | `shm:DIR`: one POSIX `shm_open` object (Windows: a named section) holding two [rings](../../protocol/ipc-transports-v1.md#ring-region) | Rust `shm_echoes`, C# `ShmEchoes`, JS ring tests (Linux `/dev/shm`), interop on all three OSes |
| Memory-mapped files | `mmap:DIR`: one mapped file holding the same two rings | `mmap_echoes` / `MmapEchoes`, JS positioned reads, interop |
| Unix domain sockets | `unix:PATH` (`@name`: Linux abstract namespace; Windows `AF_UNIX` too) | `unix_echoes`, `abstract_unix_echoes`, C# twins, JS `unix:` test, interop on Linux and macOS |
| TCP/IP loopback | `tcp:HOST:PORT` or a bare `host:port` | `tcp_echoes`, `bare_address_is_tcp`, every existing protocol test |
| UDP loopback | `udp:HOST:PORT`: per-connection socket after a `HELLO`/`WELCOME` handshake plus the [reliable session](../../protocol/ipc-transports-v1.md#reliable-datagram-session) | `udp_echoes`, `datagram_sessions_survive_loss`, JS handshake and loss tests, interop |
| Anonymous pipes | `stdio` (a server on its standard input/output) and `exec:COMMAND` (a client that starts it, no shell) | `exec_runs_a_child_over_anonymous_pipes`, `a_stdio_server_serves_one_connection_and_saves`, JS `exec:` tests, interop `exec:` from every client |
| Named pipes | `pipe:NAME`: a Windows duplex byte-mode named pipe; on POSIX `pipe:` is `fifo:`, two FIFOs | `pipe_echoes`, `fifo_echoes`, `a_second_pipe_server_is_refused`, JS FIFO tests, interop |
| Windows ALPC | `alpc:DIR`: two `\RPC Control` ports exchanging messages through `NtAlpc*` | `alpc_echoes` / `AlpcEchoes`, `alpc_carries_many_frames` / `AlpcCarriesManyFrames` and interop on Windows |
| Windows `WM_COPYDATA` | `copydata:DIR`: two message-only windows on their own message-loop threads | `copydata_echoes` / `CopydataEchoes` and interop on Windows |
| Windows mailslots | `mailslot:DIR`: one local mailslot per direction | `mailslot_echoes` / `MailslotEchoes` and interop on Windows |
| POSIX message queues | `mq:DIR`: two `mq_open` queues | `mq_echoes` / `MqEchoes` and interop on Linux |
| OS signals | `signal:DIR`: a mapped ring whose every update is announced to the peer by a signal (`SIGUSR2` by default; Windows: a named event) | `signal_echoes` / `SignalEchoes`, JS signal-ring tests, interop |
| Files on disk | `file:DIR`: a regular file holding two rings, positioned reads and writes only | `file_echoes` / `FileEchoes`, interop |
| Other similar mechanisms | `sysvmq:DIR` (System V queues), and the registry of custom kinds in all three implementations | `sysvmq_echoes`, `custom_kinds_can_be_registered` / `CustomKindsCanBeRegistered`, JS `open: custom kinds` |
| Text and binary LiNo | The transports carry bytes; neither the transports nor the relay parse LiNo | `every_local_kind_serves_queries_in_both_protocols`, `relays_stack_transports_in_both_protocols`, interop runs every kind in both protocols |
| Client and server decorators | `LinksServer::listen_all` / `LinksServer.ListenAll`, `LinksClient::connect_endpoint` / `ConnectEndpoint`, `RemoteLinks` over any endpoint, `connectIpc` | `clients_of_different_kinds_share_one_store_and_its_pushes`, `ServersListenOnSeveralKindsAndShareTheStore`, `RemoteLinksConnectToAnyEndpoint` |
| Optional and interchangeable | A bare address is still TCP; one endpoint string swaps the mechanism; nothing changes without one | All earlier tests unchanged, `endpoints_parse` / `EndpointsParse`, JS `endpoints parse as the Rust reference does` |
| Stackable | `--serve` repeats (one store, several mechanisms); `--forward` relays byte for byte with half-close, without a database | `relays_stack_transports`, `one_server_serves_one_store_on_several_endpoints`, C# `OneServerListensOnEveryServeEndpointAndARelayStacksAnotherOnTop`, interop relays each kind to the other implementation |
| Configuration | Per-endpoint options (`timeout`, `poll`, `capacity`, `chunk`, `rto`, `heartbeat`, `dead`, `signo`, `maxmsg`, ...); unknown options are an error | Endpoint parsing tests in all three implementations |
| Logging | `--trace` or `CLINK_IPC_TRACE=1` logs each transport event as `[ipc] ...` on standard error | `CliIpcEndpointsTests` asserts the log; JS `the [ipc] log goes to standard error when on` |
| Errors | Unsupported kinds, missing listeners and misused options fail explicitly and quickly | `unsupported_kinds_fail_cleanly`, `connecting_without_a_listener_fails_quickly`, `misused_forward_and_graphql_are_refused` and its C# Theory |
| Rust, C# and JavaScript agree | One written specification; C# and JS tests mirror the Rust reference test for test | [`examples/ipc/interop.mjs`](../../../examples/ipc/interop.mjs) in the [IPC workflow](../../../.github/workflows/ipc.yml) |
| Research, plans and data | This study, [PLAN.md](PLAN.md) and [`evidence/`](evidence) | Linked from PR #175 |

The interop script checks every kind of the platform in three ways:

- **Every implementation serves.** Each one (Rust, C#) serves the kind, and
  every client (Rust and C# in both protocols, JavaScript in text) writes a
  link through it. A reader then checks that the server kept every write.
- **Every implementation relays.** Each one relays the kind to the other
  implementation's TCP server, and every client talks through that relay.
- **`stdio` runs through `exec:`.** Each client starts each server through
  `exec:` and talks to it over anonymous pipes.

On Linux with the release Rust binary and the C# build, all 226 conversations
pass without JavaScript. With `--js`, the JavaScript conversations are added
and they pass too.

## Native semantics and the resulting design

The [Linux IPC overview](https://man7.org/conf/lca2013/IPC_Overview-LCA-2013-printable.pdf)
and the [Microsoft IPC guide](https://learn.microsoft.com/en-us/windows/apps/develop/communication/interprocess-communication)
give the inventory. The mechanisms come in three shapes, and the specification
turns each shape into the same stream:

- **Byte streams:** `tcp`, `unix`, Windows `pipe`, `fifo`, `stdio` and
  `exec`. They carry the protocol unchanged, and the native end-of-file ends a
  direction. A Windows named pipe cannot half-close, so ending its outgoing
  direction closes it.
- **Shared storage:** `shm`, `mmap`, `file` and `signal`. One region holds two
  single-producer rings, one per direction, so two sides that write at once
  never deadlock. Each `head`/`tail` counter is followed by its bitwise
  complement, and a reader that sees a mismatch reads both again. That lets
  `file` and the JavaScript client read the region safely with positioned
  reads and writes instead of atomics. A reader that sees the writer end loads
  the counters once more before it reports the end. This fixes a race found
  while testing, where the last bytes published together with the end could
  be lost (`0f8d6cf1`). The JavaScript ring stream also restarts its reader
  as soon as a slow consumer drains it; before `b189f69a` a stream piped into
  such a consumer could stall for good.
- **Messages:** `udp`, `mq`, `sysvmq`, `mailslot`, `copydata` and `alpc`. One
  stop-and-wait session numbers DATA and EOF frames, acknowledges a frame
  before the reader may see it, retransmits with doubling delays and
  suppresses duplicates. Like TCP's `shutdown(SHUT_WR)`, it treats the end as
  delivered when the peer hung up before its last acknowledgement arrived. A
  System V queue removed by the reader discards that acknowledgement, and a
  full POSIX queue drops it (`4720c173`). A HELLO that cannot be sent at all
  (`udp:HOST:0`) fails the JavaScript connect without leaving its resend
  timer behind (`c202de9c`).

Kinds whose address is a directory meet through a
[rendezvous directory](../../protocol/ipc-transports-v1.md#rendezvous-directory).
The listener's `listener.json` is answered by `ID.req` and `ID.ack` files,
each written as `NAME.tmp` and then renamed. Each side checks that the peer
process is still alive, so a crashed peer hangs the channel up instead of
blocking it. On POSIX the client unlinks every name once both sides have it
open, so nothing is left behind after a crash.

Per mechanism:

- **[Pipes and FIFOs](https://man7.org/linux/man-pages/man7/pipe.7.html)** are
  non-blocking and wait with `poll(2)` in slices of at most 100 ms, so a
  timeout or a hang-up interrupts them. A one-byte greeting tells "the writer
  has not opened yet" apart from end-of-file. On macOS `poll` and `kqueue`
  report a FIFO readable only while it holds bytes and never when its writer
  closes: XNU's [`filt_vnode_common`](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/vfs/vfs_vnops.c)
  counts the bytes only. The macOS CI found that `fifo` and POSIX `pipe` then
  waited for an end that `poll` never reported, so a reader tries to read
  after every slice (20 ms on macOS) whether `poll` said ready or not. A
  non-blocking FIFO write there also fails until `PIPE_BUF` bytes are free,
  though `poll` already reports the FIFO writable, so the writer sleeps 1 ms
  before it retries. The JavaScript client hit the same wall through libuv,
  which watches a FIFO with `kqueue`: on macOS every JavaScript `fifo`
  conversation waited for its one-second close fallback. There it reads the
  FIFO with `fs.read` after every wait instead (1 ms, doubling up to 20 ms
  while idle).
- **Stream sockets.** On macOS a socket shut down both ways refuses every
  `setsockopt` with `EINVAL` (XNU [`sosetoptlock`](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/uipc_socket.c)),
  so setting a read timeout on it failed the read. Rust and C# now trace the
  refused timeout and read the end anyway. On Windows `shutdown(SD_BOTH)` does
  not wake a `recv` blocked on another thread, so a stopping C# server also
  closes the socket there.
- **[System V message queues](https://man7.org/linux/man-pages/man2/msgop.2.html)**
  refuse with `EINVAL` a message longer than `msgmax` (8192 bytes on Linux) or
  than the queue holds. On macOS a queue holds only 2048 bytes by default
  (XNU `MSGMNB`), so the 2048-byte chunk the tests asked for failed every
  send there. Each side now lowers its chunk until a frame fits both queues
  (`IPC_STAT`'s `msg_qbytes`) and the system limit.
- **[Windows named pipes](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipes)**
  use overlapped byte mode with `PIPE_REJECT_REMOTE_CLIENTS`. The first
  instance takes `FILE_FLAG_FIRST_PIPE_INSTANCE`, so a second server of the
  same name fails.
- **[POSIX message queues](https://man7.org/linux/man-pages/man7/mq_overview.7.html)**
  have per-user quotas. The queue depth `maxmsg` defaults to 8, below Linux's
  default limit of 10. macOS has no POSIX queues, so it gets System V queues
  (`sysvmq`).
- **[Signals](https://man7.org/linux/man-pages/man7/signal.7.html)** coalesce
  and carry no payload. A signal only wakes the peer, and a waiting side still
  checks the ring every 100 ms or more often. The default is `SIGUSR2` because
  Node.js reserves `SIGUSR1`. On Windows a named auto-reset event plays the
  role of the signal.
- **[`WM_COPYDATA`](https://learn.microsoft.com/en-us/windows/win32/dataxchg/wm-copydata)**
  is sent with `SendMessageTimeoutW`, and the window procedure copies the frame
  before it returns. A window takes frames only when `wParam` is the peer
  window it learned in the handshake. Windows does not say which process sent
  a message, so this kind is only as private as its rendezvous directory; the
  C# and Rust sources document this.
- **[Mailslots](https://learn.microsoft.com/en-us/windows/win32/ipc/about-mailslots)**
  are local `\\.\mailslot\clink-ID-*` names, one per direction. They need no
  remote mailslot service, which [Windows 11 24H2 disables](https://learn.microsoft.com/en-us/windows/whats-new/whats-new-windows-11-version-24h2).
- **ALPC** has no documented Win32 wrapper. The structures and `NtAlpc*`
  signatures follow [System Informer's phnt](https://github.com/winsiderss/systeminformer/blob/master/phnt/include/ntlpcapi.h),
  checked against [Microsoft Terminal's ALPC client](https://github.com/microsoft/terminal/blob/main/src/interactivity/onecore/ConIoSrvComm.cpp)
  and the [ALPC samples](https://github.com/csandker/InterProcessCommunication-Samples).
  Each side owns an inbox port and connects to the peer's port. The C#
  natives use `[DllImport]` so CodeQL can analyze them. A frame sent without
  `ALPC_MSGFLG_SYNC_REQUEST` arrives as `LPC_REQUEST` with the continuation
  bit (`0x2001`), not as the `LPC_DATAGRAM` (3) that phnt's names suggest.
  Both implementations first accepted only type 3, so every frame was dropped
  and the Windows CI hung; the `[ipc]` traces of each port step showed it.
  Both types now carry a frame, and `alpc_carries_many_frames` /
  `AlpcCarriesManyFrames` send thousands of unreplied frames each way.
- **Shared memory names** are `/lcID`: 23 bytes, within Darwin's 31-byte
  `PSHMNAMLEN` limit ([`posix_shm.h`](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/posix_shm.h)).
  The first, Python design ran into that limit on the macOS runner.

## Alternatives

| Approach | Decision |
| --- | --- |
| A separate relay process in front of the TCP servers (the first iteration, in Python) | Replaced. It needs another runtime and an extra process for every hop, and it does not make the C# and Rust libraries carry the mechanisms the review asked for. Its role is kept as `clink --forward`, a decorator that stacks. |
| Libraries such as Rust [`interprocess`](https://github.com/kotauskas/interprocess) or [Boost.Interprocess](https://www.boost.org/doc/libs/latest/doc/html/interprocess.html) | Not used. They cover part of the list (sockets, pipes, shared memory, queues) but not ALPC, `WM_COPYDATA` or mailslots. Their wire formats would differ between languages. The implementations call the OS directly (`libc`/`windows-sys`, P/Invoke, Node.js `fs`/`net`/`dgram`) under one written format. |
| A message bus or RPC framework | Not used. It would replace the LiNo protocols and choose its own transport, instead of showing each named mechanism. |
| JavaScript native addons for message queues, ALPC and window messages | Not used. They would add a compiled dependency. JavaScript reaches those kinds through `clink --serve stdio --forward ENDPOINT` over anonymous pipes, so the requested mechanism still carries the bytes between the bridge and the server. |

## Lifecycle, configuration and verification

- **Bounded resources.** Rings default to 64 KiB per direction, datagram
  receivers accept at most 1 MiB that is not yet read, and every wait has a
  timeout or a hang-up check.
- **Clean shutdown.** Closing a listener ends `accept`. A stop signal closes a
  server or a relay cleanly.
- **One banner per endpoint.** `--serve` prints `clink server listening on
  ENDPOINT` once per endpoint, on standard error when one endpoint is `stdio`.
- **Security comes from the OS.** Rendezvous directories are `0700` on POSIX,
  and Windows pipes refuse remote clients. Local IPC adds no encryption or
  authentication of its own; the server's `--permissions` and tokens still
  apply.

The configuration of every kind is in the [specification](../../protocol/ipc-transports-v1.md#options),
and the runnable commands are in [`examples/ipc/`](../../../examples/ipc/README.md).

Automated coverage:

- **Rust:** 34 transport tests in `rust/src/protocol/ipc/tests.rs`, and the
  server and CLI tests in `rust/tests/protocol_ipc_tests.rs` and
  `rust/tests/cli_ipc_tests.rs`.
- **C#:** the same transport tests in `IpcTransportTests.cs`, the server tests
  in `LinksServerIpcTests.cs`, and the CLI tests in `CliIpcEndpointsTests.cs`.
- **JavaScript:** the unit tests (`js/client/test/ipc-*.test.mjs`) cover
  `src/ipc*.js` fully without `clink`. `test/ipc.test.mjs` runs against the
  Rust server.
- **The [IPC workflow](../../../.github/workflows/ipc.yml)** builds both CLIs
  on Ubuntu, macOS and Windows and runs the interop script there with the
  JavaScript client. The Rust and C# workflows run their own suites on their
  matrices.

### Dependencies and release

- **Freshness gate.** The first CI pass found dependency-freshness failures,
  which were fixed and later landed on main through PR #174.
  `cargo-audit` 0.22.2 is held under [issue #176](https://github.com/link-foundation/link-cli/issues/176).
  The releases published while this PR was open are taken here:
  Microsoft.NET.Test.Sdk 18.12.0, `rusqlite` 0.42.0 (the `sqlite` engine
  tests pass with it), and lucide-react 1.72.0, prettier 3.10.0 and vite 8.3.8
  for the workbench.
- **Changelog entries.** C# has a minor changeset, Rust has changelog
  fragments (the IPC feature is minor; the ring and datagram fixes are patch),
  and the JavaScript client is 0.5.0, which exports `./ipc`.
- **WASM lockfile.** The automated Rust 3.0.0 release left
  `rust/wasm/Cargo.lock` behind. Release automation now updates, verifies and
  stages the dependent lockfile in the same commit (`a4011624`). The
  [probe](../../../experiments/issue-140/release-lockfiles/README.md) and
  [before/after evidence](evidence/release-lockfiles.txt) record that finding.
