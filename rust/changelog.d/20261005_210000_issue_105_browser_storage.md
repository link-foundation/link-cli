---
bump: patch
---

Fixed the WebAssembly build, which broke when `NamedTypeLinks` started requiring `update_observed` and `delete_observed`. The browser workbench now runs on the new `LinkStorage::in_memory`, the CLI's store without a file, instead of its own simplified copy. In the browser, deleting a link now also deletes its usages, and updating a link into an existing pair merges the two, exactly as in the CLI.
