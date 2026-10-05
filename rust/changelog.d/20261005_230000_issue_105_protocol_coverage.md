---
bump: patch
---

`protocol::packet::address_tier` now returns a `ProtocolResult` and fails as unencodable for an address beyond the internal range instead of silently returning width 8. A server that is stopping now answers a request it already read with `(error: "server is shutting down")` instead of hanging up, and its trace notes when a client hangs up. Malformed-operation errors print the offending link in LiNo instead of Rust `Debug` syntax, and the trailing-bytes error no longer depends on the byte count. Tests now cover every line of the protocol code: each malformed-packet test asserts the exact error, which fixed cases that passed for the wrong reason, and new tests cover misshapen `name`/`link` replies, archive file errors and a stopped server.
