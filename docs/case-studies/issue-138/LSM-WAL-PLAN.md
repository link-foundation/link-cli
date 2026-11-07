# Physical LSM semantic WAL component

This is an atomic production prerequisite of `PHYSICAL-LSM-PLAN.md`. It must
not be reported as a completed physical LSM store or completed wider issue.

- [x] Freeze versioned checkpoint/manifest/group semantics with an independent
      oracle, including bounded replay, canonical u64 values, nonreused table
      reservations, positive-generation tombstones and live empty journals.
- [x] Preserve actual missing-API failures in Rust, C# and JavaScript before
      implementing the production codec. Keep runnable isolated-baseline controls.
- [x] Implement matching explicitly selected replay/checkpoint codecs in all
      three languages and include Rust controls in the actual-WASM suite.
- [x] Add independent vectors, ownership/invalid public-state controls, finite
      admission limits, public types and strict optionality checks.
- [x] Focused native Rust and C# controls pass; JS's expanded 111-vector suite
      and component line/branch/function coverage pass at 100%.
- [x] Complete full native Rust, C# Debug/Release, actual-WASM, Node/Bun/Deno,
      type, repository and optional-dependency checks; record full denominators.
- [ ] Review and commit the component independently; push only the prepared
      branch and inspect fresh exact-head CI, including all native platforms.
- [ ] Continue physical LSM provider/store composition, actual file/process
      failures, cluster integration and every other unmet acceptance criterion.

The 111-case oracle has 11 valid cases. It accepts equivalent JSON escapes but
all numeric wire fields, including manifest counts/bytes, use canonical decimal
strings. Semantic replay performs no storage I/O. Manifest-selected tables,
combined retained capacity and the complete provider namespace require physical
validation before a store may expose the result. Provider allocation bounds,
publication decisions, ownership and retirement remain separate required work.

Complete sequential validation finishes 2026-10-27T00:43:23Z: 1,075 native Rust
plus six doctests, 674 actual-WASM, 2,351 C# per configuration, 535 JS, 176
selected Bun/Deno each, all 25 native table writer/reader pairs and all 221
repository checks pass. `lsm-wal.txt` retains complete coverage denominators.
The initial C# run rejected two synchronous Facts carrying `Timeout`; async
Task-returning declarations correct that test setup. The original failure is
preserved, both focused controls and the complete configurations then pass.
