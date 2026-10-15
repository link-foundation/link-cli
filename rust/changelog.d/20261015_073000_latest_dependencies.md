---
bump: patch
---

Update doublets to 0.8.0, links-notation to 0.26.0, and lino-arguments to 0.5.0, refreshing the Rust and WebAssembly lockfiles to keep the dependency freshness gate passing.

Upstream now formats empty and control-character references with lossless `~1{…}` UTF-8 literals. Text protocol tests assert this canonical output and continue to verify reading legacy quoted multiline references.
