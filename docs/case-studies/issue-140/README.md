# Issue 140: interchangeable native IPC for both LiNo protocols

The implementation adds an optional, dependency-free Python package in
[`ipc/`](../../../ipc/README.md). It connects the existing Rust, C# and
JavaScript protocol implementations through native IPC relays. Each relay has
independent incoming and outgoing endpoints: the same component decorates a
client, a server, or another relay. The database engines and LiNo wire formats
remain authoritative.

## Collected evidence and scope

The [original issue](https://github.com/link-foundation/link-cli/issues/140)
contains thirteen named mechanisms, an open-ended request for similar IPC,
both text and binary protocols, optional interchangeable/stackable decorators
on both ends, broad configuration, and this researched case study with plans
and execution in one PR. The complete intake is preserved in
[`evidence/issue.json`](evidence/issue.json). There were no issue comments,
PR conversation comments, inline review comments, reviews, images or required
sub-issues at intake; the other JSON files preserve those empty collections
and the original PR state. [PLAN.md](PLAN.md) tracks completion and verification.

The intake baseline is main commit `199a07bb1ca4f71d06503f94ae51ac3fa46e7531`.
Main subsequently advanced to `9bf97cee`; its CI improvements and source
refactorings are merged before final verification. Its later automated Rust
3.0.0 release (`1674b770`) is also merged; the dependent WASM lockfile omission
found during that update is repaired with a reproducing release-script test.
Rust's `protocol/server.rs` accepts TCP connections and its `LinoConnection`
client owns a `TcpStream`. C#'s `LinksServer` and `LinoConnection` use TCP.
JavaScript's `connectTcp` uses `node:net`; its process adapter starts a TCP
server. These boundaries were the reason most listed mechanisms could not
be selected. Protocol framing itself did not require TCP. We studied the
recent merged [triplet protocol PR #160](https://github.com/link-foundation/link-cli/pull/160)
and [subscription PR #155](https://github.com/link-foundation/link-cli/pull/155):
retaining their parsing, sessions and independent server pushes is essential.

Platform qualification is necessary: ALPC and WM_COPYDATA are Windows
mechanisms, and POSIX message queues/signals have different availability.
An unsupported native endpoint fails explicitly. No endpoint silently sends
payload through TCP in place of its named mechanism. A local control socket
or named pipe only negotiates metadata and detects process lifetime.

“All other similar” cannot enumerate every future OS facility. We implement
every named mechanism and provide `register()` plus a documented duplex stream
contract for additional mechanisms. Notification-only facilities need a payload
component; `signal:` demonstrates that composition explicitly. This is the
scope interpretation used by this PR, rather than claiming universal support
for every OS or kernel extension.

## Requirements, solutions and verification map

The execution plan for every row was: implement a native bounded endpoint,
normalize it to a duplex stream, exercise it between independent processes,
then place relays on both sides of real text/binary protocol conversations.
`test_native_cross_process` sends all byte values, UTF-8, CRLF and framing
characters, with a payload larger than the transport slots and fragmented
reads. `test_every_native_with_text_and_binary` runs real CLI clients and
servers through each platform's endpoints. Anonymous pipes have dedicated tests.

| Requirement | Implemented solution | Verification |
| --- | --- | --- |
| Shared memory | `shm:`: native `SharedMemory`, two 4096-byte mailboxes and OS file locks | Native cross-process test; asserts actual shared-memory allocation; full-slot timeout and peer-crash regressions; both protocols |
| Memory-mapped files | `mmap:`: two backing files mapped by `mmap`, same locked mailbox framing | Native test; asserts real mapping and backing files; concurrent close regression; mixed stack and subscription pushes |
| Unix domain sockets | `unix:`: native `AF_UNIX` stream socket, private mode | POSIX native test and both protocols on Linux |
| TCP/IP loopback | `tcp:`: IPv4/IPv6 byte streams, ephemeral bind supported | Native test; entrance/exit of every real protocol test |
| UDP loopback | `udp:`: per-session datagram sockets, sequence/ACK framing, duplicate suppression and bounded receive slot | Native and protocol tests; lost ACK/deduplication regression; lost welcome with only one accept regression |
| Anonymous pipes | `exec:` starts an argv without a shell; `stdio:` serves its inherited pipes | Child-process binary-safe echo, blocked-reader cancellation, real server query/EOF |
| Named pipes | POSIX `fifo:` / `pipe:` uses two native FIFOs; Windows `pipe:` uses an actual duplex named pipe | Native tests on their respective OSes and both protocols |
| Windows ALPC | `alpc:`: paired `NtAlpc*` native ports, bounded datagrams | Independent-process ABI test and Windows C# protocol matrix |
| Windows WM_COPYDATA | `copydata:`: paired hidden windows; copies message data during callback; one receive slot | Independent Windows processes, payload larger than a slot, both protocols |
| Windows mailslots | `mailslot:`: paired local `CreateMailslotW` handles; ordered/acknowledged delivery | Independent Windows processes, both protocols; same reliability adapter as UDP |
| POSIX message queues | `mq:`: two `mq_open` queues, four bounded messages each, nonblocking operations | Linux native and protocol tests; native errors surface if kernel quotas disallow creation |
| OS signals | `signal:`: shared-memory payload plus SIGUSR1 notifications and wakeup pipe | POSIX native test; both protocols; bounded 1000-signal experiment and handler regression |
| Files on disk | `file:`: two bounded seek/read/write mailboxes with file locks, no mmap | Native test asserts file I/O storage; both protocols |
| Other similar mechanisms | Public connector/listener registration with `Stream` contract | Custom adapter registration test; duplicate/builtin registration rejected |
| Both text and binary LiNo | Relays copy bytes without parsing or packet conversion | Existing CLI count/create/delete in both protocols, including binary external references, variable arity and packed widths |
| Both client and server decorators | Identical relay command and stream API for each end | Client-side TCP-to-native plus server-side native-to-TCP in each protocol case; Rust and C# backends; existing JavaScript client example/test |
| Optional and interchangeable | Separate package, no dependency added to existing languages; endpoint selected by URI | Existing commands remain valid; same test loop selects each endpoint kind |
| Stackable | Any outgoing endpoint can target another listener, including `exec:`/`stdio:` | TCP → mmap → shared memory → TCP mixed stack, with subscription pushed back through all hops |
| Broad configuration | Listen/connect URI, timeout, poll interval, max connections, once mode, readiness file and opt-in tracing | Invalid-option tests; ephemeral-port readiness exercised by protocol orchestration; runnable examples |
| Research, plans, existing components and collected data | This study, evidence snapshots, source list and work plan | Repository artifacts linked here and in PR #175 |
| Single PR, no deferred named mechanism | All code, tests, docs and CI in [PR #175](https://github.com/link-foundation/link-cli/pull/175) | Final head CI and PR review; never merge the PR as part of this task |

## Native semantics and resulting design

IPC has three relevant shapes: byte streams, message channels and shared
storage. Notifications are a fourth role, but cannot replace arbitrary payload.
The [Linux IPC overview](https://man7.org/conf/lca2013/IPC_Overview-LCA-2013-printable.pdf)
and [Microsoft IPC guide](https://learn.microsoft.com/en-us/windows/apps/develop/communication/interprocess-communication)
provide the mechanism inventory. The common contract is `recv`, `sendall`,
directional EOF and cancellation. The relay maintains one path per direction,
so a notification does not wait for the client to issue another query.

Two independent buffers are needed for FIFO/shared-storage/message transports.
A single bidirectional mailbox can deadlock when both sides are sending.
Mailboxes use OS file locks rather than assuming atomic writes or CPU memory
ordering. A zero length marks an empty slot; `0xffffffff` marks EOF. Each side
waits for empty space before writing and consumes payload before EOF. File
endpoints are IPC, not a persistent operation log or a database backend.
Python's [shared-memory documentation](https://docs.python.org/3/library/multiprocessing.shared_memory.html)
explains the independent resource-tracker hazard; `track=False` is why this
package requires Python 3.13+. Both peers can unlink owned blocks on shutdown,
including when the creator was killed.
Darwin limits names to 31 bytes (`PSHMNAMLEN` in
[Apple's native header](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/posix_shm.h)).
The initial macOS CI run exposed longer generated names. Names now fit that
limit, including the leading slash, while retaining 96 random bits; a bounded
regression applies the native length restriction around a real allocation
and payload exchange on every platform.

[Pipes/FIFOs](https://man7.org/linux/man-pages/man7/pipe.7.html) have byte-stream
semantics and bounded kernel buffers. Paired POSIX FIFOs therefore carry framed
chunks, with separate directional EOF. Temporary anchor descriptors avoid an
open-time deadlock and are removed after the handshake. Anonymous pipes use
raw I/O with cancellation/polling and a stalled-write deadline. Only the child
created by `exec:` is terminated/reaped; no shell evaluates the argv.
[Windows named pipes](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipes)
use Python's existing native `multiprocessing.connection` implementation with
bytes-only messages, never pickle or object deserialization.
Windows `pipe:name` identifiers do not depend on a process's working directory.
A reproducing regression checks that identity, and the native Windows test
connects processes launched from different directories with the same bare URI.

[POSIX message queues](https://man7.org/linux/man-pages/man7/mq_overview.7.html)
have kernel size/count quotas and persist until unlink. Linux endpoints use
four 4097-byte messages (one tag plus a chunk) per queue, unique names and
explicit cleanup. macOS is not advertised as supporting the Linux MQ adapter.
Polling operations are nonblocking and detect peer death through the separate
control connection. Nothing mounts filesystems or changes kernel limits.

UDP provides datagrams rather than stream delivery guarantees. Our local
adapter uses a per-session random token, monotonically increasing sequence
numbers and one outstanding message. ACKs are issued only after bytes enter
the bounded receive slot. An ACK loss can repeat a datagram; the receiver ACKs
it again without delivering duplicate bytes. Handshake welcomes are repeated
from the session socket until the client responds, including with `--once`.
Heartbeats detect a silent peer. A timeout fails the session with an unknown
operation outcome; the relay never reconnects or replays database requests.
Numeric loopback addresses are enforced for UDP.

[Signals](https://man7.org/linux/man-pages/man7/signal.7.html) may coalesce and
do not carry arbitrary LiNo. SIGUSR1 is therefore a wakeup hint for the shared
mailbox, with polling as a lost/coalesced-notification safeguard. During repeated
testing a Python `Event.set()` signal handler deadlocked when interrupting a
lock-taking wait. The retained fix uses Python's native signal wakeup FD and a
nonblocking self-pipe; the Python handler takes no locks. The bounded reproducer
is [`signal-wakeup.py`](../../../experiments/issue-140/signal-wakeup.py).
Embedding explicitly opts into ownership of SIGUSR1 and the process wakeup FD.

[WM_COPYDATA](https://learn.microsoft.com/en-us/windows/win32/dataxchg/wm-copydata)
requires the receiver to copy bytes before returning from the callback; no
pointer is retained. `SendMessageTimeoutW` bounds sending. A FALSE receiver
result means the receive slot was full, allowing a retry; an API failure or
timeout has an unknown outcome and is never retried as a query. Hidden windows
run in an owned message thread and respect Windows session/integrity boundaries.

[Local mailslots](https://learn.microsoft.com/en-us/windows/win32/ipc/about-mailslots)
use `\\.\mailslot\...` only. The UDP sequence/ACK adapter is reused over native
ReadFile/WriteFile mailslot messages, so retransmission stays below LiNo and
does not duplicate application bytes. The
[Windows 11 24H2 remote-mailslot change](https://learn.microsoft.com/en-us/windows/whats-new/whats-new-windows-11-version-24h2)
does not require enabling a remote service for these local handles.

ALPC has no portable stable Win32 wrapper. Its optional 64-bit adapter uses
`ntdll` exports with explicit pointer-width ctypes signatures and structures,
following [System Informer's phnt definitions](https://github.com/winsiderss/systeminformer/blob/master/phnt/include/ntlpcapi.h).
[Microsoft Terminal's native ALPC client](https://github.com/microsoft/terminal/blob/main/src/interactivity/onecore/ConIoSrvComm.cpp)
and the [ALPC examples](https://github.com/csandker/InterProcessCommunication-Samples)
were checked to distinguish connection/communication ports and receive behavior.
Each peer owns an inbound port and an outbound connection; no handles, shared
views or impersonation are transferred. This ABI is tested on a real Windows
runner and fails explicitly on unsupported layouts/exports.

## Alternatives and reusable components

| Approach | Useful existing component | Decision |
| --- | --- | --- |
| Refactor every Rust/C#/JS protocol client/server around new native transports | Rust [`interprocess`](https://github.com/kotauskas/interprocess), .NET pipes/sockets | Possible, but duplicates native lifecycle/framing work across languages and still needs custom ALPC, WM_COPYDATA and mailslots. The optional relay reuses every existing protocol implementation now. |
| C++ native transport service | [Boost.Interprocess](https://www.boost.org/doc/libs/latest/doc/html/interprocess.html) supplies shared memory, mapped files, synchronization and message queues | Useful reference, but adds a compiled dependency/toolchain and still does not provide every requested Windows primitive. |
| A message-bus/RPC library | Existing transports can be wrapped through `register()` | Such libraries change deployment and framing and may substitute their own transport. They cannot demonstrate each named primitive without native adapters. |
| Dependency-free shared relay | Python sockets, mmap, SharedMemory, subprocess and multiprocessing named pipes; ctypes for POSIX MQ/Windows natives | Selected. One bounded stream contract, one relay implementation, portable packaging, no change to existing engine dependencies. |

The cost of the selected design is an optional Python runtime and extra relay
processes at existing TCP boundaries. It is not a zero-copy or latency claim.
Direct Python embedding can omit either TCP boundary. Benchmarking every OS
mechanism is outside this feature's acceptance criteria; the bounded queues,
poll interval and number of relays are exposed/documented so users can choose.

## Lifecycle, configuration and verification

[`ipc/README.md`](../../../ipc/README.md) documents every URI and option,
platform constraints, security scope, extension API and stack examples.
[`examples/ipc`](../../../examples/ipc/README.md) includes standalone commands
and an existing JavaScript client. Buffers and connection workers are bounded;
each accepted session owns its resources. Fresh private endpoint directories
prevent deleting preexisting data. Normal close removes owned paths/handles;
after an uncatchable kill a stale endpoint directory requires operator cleanup.
Windows listener cleanup waits, within its configured deadline, for peer-held
mapping/file handles to close before removing the endpoint directory. CI caught
the EOF-versus-handle-release race; a test deliberately holds those handles
open while the listener closes, then releases them and verifies cleanup.
This is local IPC security inherited from OS ownership/ACLs, not encryption or
a replacement for the server's existing authentication/permissions.

Focused validation does not build Rust or C# locally:

```sh
PYTHONPATH=ipc python -m unittest discover -s ipc/tests -v
PYTHONPATH=ipc python experiments/issue-140/signal-wakeup.py
node --test .github/scripts/pipeline-status.test.mjs
bash experiments/issue-140/release-lockfiles/run.sh
```

The [IPC workflow](../../../.github/workflows/ipc.yml) runs native independent
process tests on Ubuntu, macOS and Windows. Provisioned CI runners build Rust
on Linux and C# on Linux/Windows and execute real CLI conversations, JavaScript
client calls, anonymous-pipe queries and mixed-layer subscriptions. Per-test
diagnostic watchdogs bound hangs; job deadlines also include source builds.
Regression tests cover interrupted cleanup, occupied mailboxes, peer crashes,
lost datagrams/ACKs/handshakes, and existing endpoint preservation. Source
quality and all repository workflows remain required on the final PR head.
Anonymous-pipe integration tests wait for `--once` relays to exit and reap
their children before deleting logs. This avoids a Windows cleanup race from
force-terminating a parent whose child still inherited its log handle; the
native suite also verifies normal relay exit releases that inherited handle.

The first CI pass also found existing dependency-freshness failures: cargo-audit,
System.CommandLine, Microsoft.Data.Sqlite, Microsoft.NET.Test.Sdk and the locked
rustls-pki-types. A subsequent registry check also required webpki-roots 1.0.12.
The dependency refreshes were made in separate commits and subsequently landed
on main through PR #174. The merged security workflow retains verified
cargo-audit 0.22.2 under its explicit [issue #176 hold](https://github.com/link-foundation/link-cli/issues/176),
since the pinned installer cannot yet verify 0.23.0. Rust and C# each have a
new patch fragment so release automation updates versions, lockfiles and
changelogs together. Python IPC starts at version 0.1.0.
CI logs were downloaded for diagnosis rather than assuming an old passing run
validated a newer commit. Final verification is recorded in [PLAN.md](PLAN.md).

After all ten workflows passed on `40522c14`, the final main fetch found the
automatic Rust 3.0.0 release. Its native lockfile was updated, but the separate
WASM lockfile still recorded core 2.1.2. The existing dependency-boundary check
reproduced the `cargo tree --locked` failure without building the engine. The
release staging regression then failed before synchronizing the dependent lock.
Release automation now updates, verifies and stages both lockfiles in the same
commit; all 15 helper tests pass. The bounded helper build peaked at 284 MiB RSS.
The [reusable probe](../../../experiments/issue-140/release-lockfiles/README.md)
and [before/after evidence](evidence/release-lockfiles.txt) preserve this finding.
Final commit verification remains recorded in the PR description.
