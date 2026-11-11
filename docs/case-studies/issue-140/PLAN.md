# Issue 140 work plan

Objective: carry the LiNo text and binary protocols over every requested IPC
mechanism as decorators in C# and Rust, usable from JavaScript, in PR #175.
Apply each change everywhere it belongs, and pass every CI check. Never merge
the PR.

## Requirements inventory

From the issue:

- [x] Shared memory: `shm`.
- [x] Memory-mapped files: `mmap`.
- [x] Unix domain sockets: `unix`.
- [x] TCP/IP loopback: `tcp`.
- [x] UDP loopback: `udp`.
- [x] Anonymous pipes: `stdio` and `exec:`.
- [x] Named pipes: Windows `pipe`, plus POSIX `fifo`.
- [x] Windows ALPC: `alpc`.
- [x] `WM_COPYDATA`: `copydata`.
- [x] Mailslots: `mailslot`.
- [x] POSIX message queues: `mq`.
- [x] OS signals: `signal`.
- [x] Files on disk: `file`.
- [x] All other similar mechanisms: `sysvmq` and a registry of custom kinds in
  Rust, C# and JavaScript.
- [x] Both the text and the binary protocol on every kind.
- [x] Optional, interchangeable decorators on the client and the server side.
- [x] Stackable: several `--serve` endpoints per store, and `--forward` relays.
- [x] Case study in `docs/case-studies/issue-140` with research, plans and data.

From the owner's PR review:

- [x] Full implementation in C# and in Rust, as decorators of their
  `LinksServer` / `LinksClient` / `RemoteLinks`.
- [x] Usable from JavaScript: `@link-foundation/link-cli-client/ipc`, native or
  through the `clink` bridge.
- [x] Correct and consistent: one written specification
  (`docs/protocol/ipc-transports-v1.md`), and interop of every kind between
  every implementation on Linux, macOS and Windows.
- [x] Validated and tested: Rust and C# transport, server and CLI tests that
  mirror each other; JavaScript unit tests covering `src/ipc*.js` fully, plus
  tests against the Rust server.
- [x] Documented: the specification, both CLI READMEs, the root README,
  `docs/ARCHITECTURE.md`, the JavaScript client README and types,
  `examples/ipc`, and this study.
- [x] Logged: `--trace` / `CLINK_IPC_TRACE=1` traces every transport event in
  all three implementations.
- [x] Applied in every place: C# CLI parity with Rust (`--serve` repeats,
  `--forward`, `--connect` to any endpoint, `--trace`). The Python relay
  package, which the decorators replace, is removed together with its CI,
  docs and experiment.
- [x] Versions and changelogs: Rust fragments, a C# changeset, and the
  JavaScript client at 0.5.0.
- [ ] Every CI check passes on the final head. This is recorded in the PR
  description.

## Execution order

1. Specify the wire contract first: endpoints, the rendezvous directory, ring
   regions and the reliable datagram session.
2. Write the Rust reference. Start with the POSIX kinds, then add the Windows
   kinds, then the server, client and CLI (`--serve` repeats, `--forward`,
   `--connect`).
3. Port everything to C#, kind for kind and test for test, then do the same
   for the CLI.
4. Write the JavaScript client: native kinds, the bridge for the rest, types
   and examples.
5. Run cross-implementation interop for every kind on all three OSes in CI.
6. Remove the superseded Python relay. Update every document, changelog and
   this study.
7. Merge main, review the full diff, and make sure every workflow passes on
   the final head.
8. Mark the PR ready.

## Verification record

- **Linux interop.** `examples/ipc/interop.mjs` with the release Rust binary
  and the C# build passed 226 of 226 conversations. Each kind was served by
  Rust and by C#, written by every client in both protocols, relayed through
  the other implementation, and run as `stdio` through `exec:`. The
  JavaScript client passed every kind it reaches, the bridge kinds included
  (with `CLINK_BIN`).
- **Rust/C# interop by hand.** Checked over `mq`, `fifo` (binary) and a `udp`
  relay.
- **C# CLI tests.** All 67 pass, including the new `CliIpcEndpointsTests`.
- **Found while testing and fixed with regression tests:**
  - A ring reader could miss the bytes published with the end.
  - A lost last acknowledgement could fail a half-close.
  - The JavaScript ring hang-up differed from Rust and C#.
  - A JavaScript ring stream piped into a slow consumer could stall.
  - A JavaScript `udp:HOST:0` connect left a resend timer that threw later.
  - A macOS FIFO reader never saw the end, because XNU `poll` does not report it.
  - A `sysvmq` chunk longer than a queue holds (2048 bytes on macOS, `msgmax`
    on Linux) failed every send; it is now lowered to fit.
  - The JavaScript client never saw a macOS FIFO end either (libuv watches it
    with `kqueue`); it now reads after every wait there.
  - A macOS socket shut down both ways refused the read timeout with `EINVAL`.
  - A stopping C# server on Windows left a `recv` blocked on another thread.
  - ALPC dropped every frame: one sent without awaiting a reply arrives as
    `LPC_REQUEST`, not `LPC_DATAGRAM`.
- **CodeQL** needs `[DllImport]` declarations for the C# natives. The WASM
  lockfile is refreshed along with the release fix.
- **Windows and macOS** are verified by the IPC, Rust and C# workflow matrices
  on the final head (see the PR description for the run links).
