---
'Foundation.Data.Doublets.Cli': minor
---

Added `--export-binary <PATH>` (aliases `--binary-output`, `--binary-out`) and `--import-binary <PATH>` (aliases `--binary-input`, `--binary-in`), which write and read the whole store, names included, as a store archive in binary links notation: a links packet that keeps every address and hole, followed by a names packet. The library exposes the same as `StoreArchive.Export`, `StoreArchive.Import`, `StoreArchive.ExportToFile` and `StoreArchive.ImportFromFile`. A binary archive is imported before the `--in` LiNo file and exported wherever `--out` is written. The archive bytes match the Rust port.
