---
bump: patch
---

### Fixed

- A GraphQL subject can now write a value on a link it has just created. `insert_links_one` with a nested `string`, `number` or `object` used to answer `Forbidden` under a permission policy, because the check on the owner of the value did not know which links the subject had created.
