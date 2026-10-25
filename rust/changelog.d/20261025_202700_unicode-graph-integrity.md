---
bump: patch
---

Reject cycles and missing nodes while decoding persisted Unicode sequences.
Preserve repeated shared subtrees, UTF-16 code-unit extraction and valid deep
graphs. Add a checked sequence walker; retain the existing Vec API with bounded
failure for malformed graphs. Keep name and metadata errors fallible.
