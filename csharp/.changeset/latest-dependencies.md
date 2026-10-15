---
'Foundation.Data.Doublets.Cli': patch
---

Update Links.Notation to 0.26.0, System.CommandLine to 2.0.13, and the test SDK and runner to their latest releases.

Upstream now formats empty and control-character references with lossless `~1{…}` UTF-8 literals. Text protocol tests assert this canonical output and continue to verify reading legacy quoted multiline references.
