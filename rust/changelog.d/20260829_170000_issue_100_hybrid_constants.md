---
bump: patch
---

Fixed Rust name lookups losing links whose external reference collided with a `doublets` service constant. `LinkStorage` now reports the hybrid `LinksConstants`, which reserve the upper half of the address range for external references, and its inherent `get_or_create` no longer resolves to `Doublets::search` — which treats `any` as a wildcard — through the `Doublets` impl for `&mut LinkStorage`. Reserved pinned type names such as `Type` or `UnicodeSymbol` also resolve deterministically now that name holders are ordered by address instead of by hash map iteration order.
