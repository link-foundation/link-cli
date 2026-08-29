---
bump: patch
---

Fixed address allocation in the Rust storage so that it matches the C# store: a freed address is reused before the store grows, the most recently freed one first, deleting the last link shrinks the store, and ensuring an address gives back the addresses passed over on the way to it. The free list is persisted so the order survives between CLI invocations.
