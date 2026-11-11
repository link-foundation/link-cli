---
bump: patch
---

### Fixed

- Datagram IPC sessions (`udp`, `mq`, `sysvmq`, `mailslot`, `copydata`, `alpc`) acknowledge an in-order DATA or EOF frame before the reader can see it, and a half-close whose end the peer never acknowledged because it already hung up now succeeds like TCP's `shutdown(SHUT_WR)`. Before, a reader that hung up right after reading the end (removing a System V queue discards a pending acknowledgement, a full POSIX queue drops it) made the writer's half-close fail with "the peer no longer reads" although every byte was delivered (#140).
- Failing to disable Nagle's algorithm on a TCP IPC stream is traced instead of being silently ignored (#140).
