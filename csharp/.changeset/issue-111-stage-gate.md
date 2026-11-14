---
'Foundation.Data.Doublets.Cli': patch
---

C# CI jobs now start only after the JavaScript stage (lint, `npm run test:js`, parity check) passes for the same commit. The C# suite loads the shared golden fixtures in `parity/golden-fixtures.json` and CI checks its receipt (issue #111).
