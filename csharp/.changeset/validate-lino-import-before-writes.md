---
'Foundation.Data.Doublets.Cli': patch
---

Parse and validate the whole LiNo input before changing links or names, so malformed imports leave the database unchanged through both the CLI aliases and the library APIs.
