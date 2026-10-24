---
'Foundation.Data.Doublets.Cli': patch
---

Preserve literal backslashes and multiline quoted names during LiNo import. Quote
backslash-containing names on export, and use the existing UTF-8 hexadecimal
reference literals for control characters and mixed quotes so user text survives
import/export/import into a fresh database.
