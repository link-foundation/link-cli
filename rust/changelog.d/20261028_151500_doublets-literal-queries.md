---
bump: patch
---

Fixed `DoubletsStorage` queries missing null endpoints and duplicate pairs.
Search and get-or-create now match literal endpoints, including zero, while
ordinary indexed searches retain their existing path. Shared native/WASM
contracts cover heap adoption, observed cascades and transaction rollback.
