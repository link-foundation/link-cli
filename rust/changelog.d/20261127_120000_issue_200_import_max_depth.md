---
bump: minor
---

Added `--in-max-depth <N>` (1 to 128, default 64) to set the nesting limit of
LiNo imports through `--in`, `--import` and `--lino-input`, and the
`import_lino_text_with_max_depth`/`import_lino_file_with_max_depth` library
functions. The 64-level default is now documented for imports (#200).
