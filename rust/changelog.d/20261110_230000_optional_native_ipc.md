---
bump: patch
---

### Added

- Optional native IPC decorators for existing LiNo text/binary clients and servers through the standalone `ipc/` package, with interchangeable, stackable transports, configuration examples and cross-platform regression coverage (#140).

### Fixed

- Keep the dependent WASM lockfile in the same verified release commit as the native manifest and lockfile, preventing `--locked` CI failures after automatic releases.
