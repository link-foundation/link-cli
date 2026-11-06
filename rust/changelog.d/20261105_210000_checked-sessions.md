---
bump: patch
---

- Preserve checked query admission and allocation reservations through
  permission and undo sessions, retaining generic addresses and original-query
  envelopes with subscription change observations.
- Upgrade to doublets 0.21.0, links-notation 0.31.3 and current released
  protocol/storage dependencies.

### Fixed

- A mapped `grow` or `shrink` of values with destructors that hits a file quota
  or mapping limit now returns the error with its view still readable. It used
  to crash on the next read.
- `shrink` now aborts if a tail destructor panics. It used to drop those values
  a second time.
