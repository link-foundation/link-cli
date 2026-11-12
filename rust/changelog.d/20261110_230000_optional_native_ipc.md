---
bump: minor
---

### Added

- `link_cli::protocol::ipc` carries the LiNo text and binary protocols over every IPC mechanism of IPC transports v1 (`docs/protocol/ipc-transports-v1.md`): TCP and UDP loopback, Unix domain sockets, Windows named pipes, POSIX FIFOs, anonymous pipes (`stdio` and `exec:`), shared memory, memory-mapped files, files on disk, OS signals, POSIX and System V message queues, mailslots, `WM_COPYDATA` and ALPC. An endpoint string selects the mechanism, `LinksServer` and `LinksClient` take any endpoint, `register` adds custom kinds, and every kind interoperates with the C# `clink` and the JavaScript client (#140).
- `clink --serve` repeats to serve one store on several endpoints at once, `--forward` relays every connection of the `--serve` endpoints to another endpoint without opening a database (so transports stack), `--connect` takes any endpoint and `--trace` (or `CLINK_IPC_TRACE=1`) logs every transport event (#140).

### Changed

- Update the optional `rusqlite` dependency of the `sqlite` feature to 0.42.0.
- Update `errno` to 0.3.15 and the optional `ureq` dependency of the `etcd` feature to 3.5.2.

### Fixed

- Drop `package.homepage`, which repeats `package.repository`; newer Cargo warns `cargo::redundant_homepage` about it, and CI denies manifest warnings.
- Keep the dependent WASM lockfile in the same verified release commit as the native manifest and lockfile, preventing `--locked` CI failures after automatic releases.
