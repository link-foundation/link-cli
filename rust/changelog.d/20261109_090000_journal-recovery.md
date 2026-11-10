---
bump: minor
---

### Added

- `engine::recovery`: `check_journal` reports the longest prefix of a journal that `Engine::open` would replay and the first damaged record; `recover_journal` picks the longest valid prefix among copies (the damaged primary, replicas, mirrors, a restored backup) and refuses copies that diverge; `repair_journal_file` rewrites a damaged file journal under the writer lock, first keeping the original bytes in `<path>.damaged-<n>`; `read_journal_file_lossy` reads a journal with invalid UTF-8 so the checksum reports it.
