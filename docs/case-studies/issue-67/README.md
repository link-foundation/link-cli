# Issue 67 Case Study: Rust Implementation Parity

Source issue: <https://github.com/link-foundation/link-cli/issues/67>

This case study captures the requirements from issue 67, the external components checked while planning the work, and the implementation path for bringing the Rust CLI into parity with the C# implementation.

## External Components Reviewed

As of 2026-04-30:

- `linksplatform/doublets-rs`: GitHub repository <https://github.com/linksplatform/doublets-rs>, latest GitHub release `v0.3.0`; crates.io package appears as `doublets = "0.3.0"`.
- `link-foundation/links-notation`: GitHub repository <https://github.com/link-foundation/links-notation>, latest GitHub release `0.13.0_csharp`; crates.io package appears as `links-notation = "0.13.0"`.
- `linksplatform/Data.Doublets.Sequences`: GitHub repository <https://github.com/linksplatform/Data.Doublets.Sequences>, latest GitHub release `csharp_0.6.5`.
- CI/CD templates requested by the issue:
  - <https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template>
  - <https://github.com/link-foundation/rust-ai-driven-development-pipeline-template>
  - <https://github.com/link-foundation/js-ai-driven-development-pipeline-template>

## Requirement Inventory

| Requirement | Current status | Solution plan |
| --- | --- | --- |
| Use the latest `doublets-rs` and `links-notation` as a Rust basis. | The current Rust package still uses local storage/parser components. The current published Rust crates are `doublets = "0.3.0"` and `links-notation = "0.13.0"`. | Introduce these crates behind small adapter modules so CLI behavior remains stable while storage and notation parsing are swapped incrementally. Start with parser fixtures, then storage fixtures, then binary compatibility fixtures. |
| Reimplement sequence support in pure Rust based on `Data.Doublets.Sequences`. | The Rust implementation has link storage and query operations, but no dedicated sequence layer matching the C# package. | Port sequence primitives as a separate Rust module with C# fixture parity tests for creation, traversal, Unicode text, deletion, and persistence. |
| Match C# Unicode support and binary file compatibility. | Existing Rust tests cover named links and persistence basics. Cross-runtime binary fixtures are not yet complete. | Add C#-generated binary fixtures and Rust-generated binary fixtures, then verify both runtimes can read each file without data loss. Include non-ASCII names and multi-codepoint text cases. |
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

The Rust test suite now includes focused parity tests in `rust/tests/query_processor_csharp_parity_tests.rs`.

## Next Parity Work

1. Add dependency adapter experiments for `doublets` and `links-notation` without replacing public CLI behavior in a single step.
2. Build cross-runtime fixture tests for binary file compatibility and Unicode names/text.
3. Port sequence primitives from the C# sequence package into Rust with fixture-driven tests.
4. Expand Rust CLI tests until every C# CLI behavior has a corresponding Rust assertion.
5. Run a workflow-template audit against the requested C#, Rust, and JS pipeline templates and apply only concrete drift fixes.
