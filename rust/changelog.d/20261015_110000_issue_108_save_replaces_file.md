---
bump: patch
---

Saving a database now writes a new file next to it and moves it over the old one instead of truncating the file and rewriting it in place. Stopping `clink --serve` (Ctrl+C, a crash) during a write used to leave an empty or partial database and lose every link, the acknowledged ones included; it now leaves the last complete save. Nothing is synced to disk, so a power loss can still lose recent writes.
