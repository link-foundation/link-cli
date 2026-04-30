# Issue 67 Case Study: Rust Implementation Parity

Source issue: <https://github.com/link-foundation/link-cli/issues/67>

This case study captures the requirements from issue 67, the external components checked while planning the work, and the implementation path for bringing the Rust CLI into parity with the C# implementation.

## External Components Reviewed

As of 2026-04-30:

- `linksplatform/doublets-rs`: GitHub repository <https://github.com/linksplatform/doublets-rs>, latest GitHub release `v0.3.0`; crates.io package appears as `doublets = "0.3.0"`.
- `link-foundation/links-notation`: GitHub repository <https://github.com/link-foundation/links-notation>, latest Rust tag `rust_0.13.0`; crates.io package appears as `links-notation = "0.13.0"`.
- `link-foundation/lino-arguments`: GitHub repository <https://github.com/link-foundation/lino-arguments>, latest Rust release `0.3.0`; crates.io package appears as `lino-arguments = "0.3.0"`.
- `linksplatform/Data.Doublets.Sequences`: GitHub repository <https://github.com/linksplatform/Data.Doublets.Sequences>, latest GitHub release `csharp_0.6.5`.
- CI/CD templates requested by the issue:
  - <https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template>
  - <https://github.com/link-foundation/rust-ai-driven-development-pipeline-template>
  - <https://github.com/link-foundation/js-ai-driven-development-pipeline-template>

## Requirement Inventory

| Requirement | Current status | Solution plan |
| --- | --- | --- |
| Use the latest `doublets-rs`, `links-notation`, and `lino-arguments` as a Rust basis. | `rust/Cargo.toml` now declares `doublets = "0.3.0"`, `links-notation = "0.13.0"`, and `lino-arguments = "0.3.0"` with upstream source links. The Rust parser delegates to `links-notation`, CLI parsing uses `lino-arguments`, and the local link model has conversion coverage for `doublets::Link<u32>`. | Continue replacing local storage internals behind compatibility tests so public CLI behavior remains stable while binary storage parity is developed. |
| Reimplement sequence support in pure Rust based on `Data.Doublets.Sequences`. | Rust now has `rust/src/unicode_string_storage.rs`, a doublet-backed port of the C# `UnicodeStringStorage<uint>` path: pinned type links, raw-number Unicode symbols, balanced Unicode sequence trees, right-sequence walking, string links, and name links. | Continue extending this module toward full package coverage for advanced sequence indexes, compaction, and binary fixture compatibility. |
| Match C# Unicode support and binary file compatibility. | Rust now round-trips empty, ASCII, multilingual, and surrogate-pair text through UTF-16 code units, matching the C# `string`/`char` model used by `Data.Doublets.Sequences`. Cross-runtime binary fixtures are not yet complete. | Add C#-generated binary fixtures and Rust-generated binary fixtures, then verify both runtimes can read each file without data loss. Include non-ASCII names and multi-codepoint text cases. |
| Support the same CLI options, features, and tests as C#. | The repository already has C# and Rust test suites. This PR closes concrete query semantics gaps found against the C# `AdvancedMixedQueryProcessor` behavior. | Continue converting C# tests into Rust parity tests by feature area: storage, parser, query processor, CLI commands, persistence, and sequences. |
| Keep C# under `./csharp`, Rust under `./rust`, and provide separate workflows. | The repository already has `csharp/`, `rust/`, `.github/workflows/csharp.yml`, and `.github/workflows/rust.yml`. | Preserve this layout. Treat future parity work as package-local changes unless a shared workflow or script must change. |
| Compare CI/CD templates and reuse best practices. | Rust and C# workflows exist, and Rust has changelog fragment based release automation. | Audit the requested templates in a follow-up pass focused on workflow drift: permissions, cache keys, test matrix, linting, changelog validation, release trigger, and artifact publishing. |
| Collect issue data in `./docs/case-studies/issue-67`. | This document satisfies the requested repository-local case study folder. | Keep this document updated as additional parity gaps are discovered or closed. |
| Plan and execute in one pull request. | PR 68 is the working pull request for this issue branch. | Keep all issue-67 implementation, tests, documentation, and release notes in PR 68. |

## Implemented In This PR

This PR focuses on query processor parity gaps that were blocking Rust behavior from matching C# query semantics:

- Accepts the unwrapped query form used by C# examples: `restriction substitution`.
- Deletes all links that match a structural restriction pattern instead of only deleting explicit link IDs.
- Supports wildcard and variable matching across nested link patterns.
- Applies variable-driven swaps and replacements using solution bindings from the restriction side.
- Returns matched changes for no-op variable substitutions, matching the C# behavior.
- Reuses existing structural links for named composite substitutions before applying a new name, avoiding accidental duplicate leaf creation.
- Declares and compiles the requested Rust basis crates: `doublets`, `links-notation`, and `lino-arguments`.
- Uses `links-notation` for parsing, including richer quoted Unicode identifiers, and uses `lino-arguments` as the CLI argument parser entrypoint.
- Adds a Rust `UnicodeStringStorage` implementation based on the C# `Data.Doublets.Sequences` path:
  - `PinnedTypes` deterministic type allocation for `Type`, `UnicodeSymbol`, `UnicodeSequence`, `String`, `EmptyString`, and `Name`.
  - `Hybrid<uint>`-compatible external/raw number encoding for Unicode code units and external references.
  - `BalancedVariantConverter`-style sequence tree creation and `RightSequenceWalker`-style traversal.
  - `NamedLinks` behavior for internal links and external references, including removal.

The Rust test suite now includes focused parity tests in `rust/tests/query_processor_csharp_parity_tests.rs` and `rust/tests/unicode_string_storage_tests.rs`.

## C# To Rust Tree Comparison

| C# file | Rust counterpart | Status |
| --- | --- | --- |
| `AdvancedMixedQueryProcessor.cs` | `rust/src/query_processor.rs`, `rust/tests/query_processor_csharp_parity_tests.rs` | Implemented for the currently tested advanced mixed query semantics, including structural matching, variables, wildcard deletes, no-op reads, swaps, and named composite renames. |
| `BasicQueryProcessor.cs` | `rust/src/query_processor.rs` | Covered by the shared Rust query processor for create, update, delete, and read scenarios. |
| `MixedQueryProcessor.cs` | `rust/src/query_processor.rs` | Covered by the shared Rust query processor and parity tests for mixed restriction/substitution behavior. |
| `ChangesSimplifier.cs` | `rust/src/changes_simplifier.rs`, `rust/tests/changes_simplifier_tests.rs` | Implemented and tested. |
| `UnicodeStringStorage.cs` | `rust/src/unicode_string_storage.rs`, `rust/tests/unicode_string_storage_tests.rs` | Implemented in this pass for pinned types, UTF-16 Unicode sequences, string links, type names, user types, external-reference names, and removal. |
| `PinnedTypes.cs` | `rust/src/pinned_types.rs`, `rust/tests/unicode_string_storage_tests.rs` | Implemented in this pass. |
| `NamedLinks.cs` | `rust/src/unicode_string_storage.rs`, existing `LinkStorage` name APIs, `rust/tests/unicode_string_storage_tests.rs` | Implemented in this pass for the doublet-backed Unicode storage path; existing CLI-facing name APIs remain compatible with prior Rust query tests. |
| `NamedLinksDecorator.cs` | `rust/src/link_storage.rs`, `rust/src/query_processor.rs` | Partially represented by `LinkStorage` plus query processor name handling. The new Unicode storage module provides the C# name database primitives needed for deeper integration. |
| `SimpleLinksDecorator.cs` | `rust/src/link_storage.rs` | Represented by direct storage create/update/delete/query methods. |
| `LinksExtensions.cs` | `rust/src/link_storage.rs` | Represented by `ensure_created` and explicit-index update paths. |
| `EnumerableExtensions.cs` | Rust destructuring is native pattern syntax; no runtime counterpart required. | Not required as a separate module. |
| `ILinksUnrestricted.cs` | No direct Rust trait yet. | Placeholder C# interface only; add a Rust trait if a future storage adapter needs this abstraction. |
| `Program.cs` | `rust/src/main.rs` | Implemented with matching CLI option aliases and query flow. |

## Next Parity Work

1. Replace the remaining local text-file storage internals with a `doublets`-backed adapter behind compatibility tests.
2. Build cross-runtime fixture tests for binary file compatibility and Unicode names/text.
3. Port sequence primitives from the C# sequence package into Rust with fixture-driven tests.
4. Expand Rust CLI tests until every C# CLI behavior has a corresponding Rust assertion.
5. Run a workflow-template audit against the requested C#, Rust, and JS pipeline templates and apply only concrete drift fixes.
