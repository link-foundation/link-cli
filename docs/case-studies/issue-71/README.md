# Issue 71 Case Study: Documentation Coverage

Issue: https://github.com/link-foundation/link-cli/issues/71
Pull request: https://github.com/link-foundation/link-cli/pull/72

## Evidence Collected

- `evidence/issue-71.json`: issue title, body, timestamps, and metadata.
- `evidence/issue-71-comments.json`: issue comments. This issue had no
  comments when the evidence was collected.
- `evidence/pr-72.json`: placeholder PR state before the documentation update.
- `evidence/pr-72-conversation-comments.json`,
  `evidence/pr-72-review-comments.json`, and `evidence/pr-72-reviews.json`:
  PR feedback endpoints. They were empty when collected.
- `evidence/issues-all.json`: all repository issues returned by GitHub CLI.
- `evidence/pulls-all.json`: all repository PRs returned by GitHub CLI.
- `evidence/repository-issue-and-pr-conversation-comments.json`: repository
  issue and PR conversation comments.
- `evidence/repository-pr-review-comments.json`: repository-level PR review
  comments. It was empty when collected.
- `evidence/code-search-named-types.json` and
  `evidence/code-search-persistent-transformation.json`: GitHub code search
  evidence for related repository terms.
- `evidence/csharp-help.txt` and `evidence/rust-help.txt`: local help output
  used to compare command surfaces.
- `evidence/named-reference-examples.txt`: local C# CLI runs proving named
  create, read, update, delete, and names sidecar behavior.
- `evidence/cargo-search-doublets.txt` and
  `evidence/cargo-search-links-notation.txt`: crate search evidence.
- `evidence/npm-doublets-web*.json`: npm metadata for `doublets-web`.
- `evidence/npm-vite-plugin-react.json`: npm metadata for a browser workbench
  dependency.
- `evidence/nuget-search-clink.txt`: attempted `dotnet nuget search clink`.
  The installed .NET SDK does not provide a `dotnet nuget search` command, so
  this file records the command limitation.

## Online Sources Checked

- NuGet `clink`: https://www.nuget.org/packages/clink
- npm `doublets-web`: https://www.npmjs.com/package/doublets-web
- docs.rs `doublets`: https://docs.rs/doublets/latest/doublets/
- docs.rs `links-notation`: https://docs.rs/crate/links-notation/0.13.0

The online package data showed one doc drift to avoid: the repository lockfile
pins `doublets-web@0.1.2`, while npm metadata observed during this work reported
`0.1.3` as latest. The WebAssembly docs now describe the committed lockfile
state instead of calling `0.1.2` the latest release.

## Requirements from Issue 71

- Make `README.md` fully show supported features.
- Make the README factually reflect the code.
- Add named-reference examples equivalent to the numbered-reference examples.
- Preserve existing documentation unless it is inaccurate.
- Add deeper docs where needed, specifically `docs/HOW-IT-WORKS.md`,
  `docs/REQUIREMENTS.md`, and `docs/ARCHITECTURE.md`.
- Compile repository issue and PR data under `docs/case-studies/issue-71`.
- Use that data for a case-study analysis.
- List requirements and propose solution plans for each requirement.
- Check known existing components or libraries that solve or support similar
  work.
- Execute everything in one pull request.

## Documentation Gaps Found

- Named references were present in a few examples, but they did not have the
  same create/read/update/delete coverage as numbered references.
- `--import` was implemented but absent from the README options table.
- `--structure` existed but lacked a dedicated user-facing example.
- The README did not explain names and triggers sidecar files.
- The README did not distinguish the complete C# NuGet command surface from the
  Rust core command surface.
- The WebAssembly docs described `doublets-web@0.1.2` as the latest release,
  which was no longer a stable statement.
- Requirements and architecture were spread across issues and PR comments rather
  than summarized in repository docs.

## Solution Plan Applied

| Requirement | Plan | Result |
|-------------|------|--------|
| README feature coverage | Add missing sections while preserving existing examples. | Added documentation links, implementation status, named references, structure, import, database files, and option notes. |
| Named-reference examples | Reproduce local C# CLI behavior and document observed outputs. | Added named create/read/update/delete/variable examples and saved logs in evidence. |
| Factual command surface | Compare C# and Rust help output. | Added C# versus Rust support notes and the missing `--in` option row. |
| Deep docs | Create requested docs. | Added `docs/REQUIREMENTS.md`, `docs/ARCHITECTURE.md`, and `docs/HOW-IT-WORKS.md`. |
| Case-study evidence | Save GitHub, package, and local CLI evidence. | Added `docs/case-studies/issue-71/evidence/` files and this case study. |
| Existing components/libraries | Identify current dependencies and online package facts. | Documented Platform.Data.Doublets, Link.Foundation.Links.Notation, Rust `doublets`, `links-notation`, `doublets-web`, React, Vite, and wasm-pack usage. |

## Requirement Summary from Repository History

Implemented requirements include:

- Substitution-based CRUD.
- Change output, before/after output, and no-op read output.
- Explicit link indexes.
- Reference validation and optional auto-create.
- Variables, wildcards, and deep patterns.
- Named references with sidecar storage.
- Deduplication of repeated links and nested sub-links.
- LiNo import and export.
- Structure formatting.
- C# persistent transformation triggers.
- Rust parity for the core query engine and browser use.
- WebAssembly browser workbench.
- Split CI workflows for C#, Rust, and WebAssembly.

Open requirements include:

- Cross-platform quote unification.
- LiNo protocol server mode.
- REST, GRPC-style, GraphQL-style, SPARQL/RDF, and SQL/PostgreSQL-compatible
  APIs.
- Benchmarks for CLI versus server and API transports.
- MCP support for neural network memory.

## Residual Risks

- README examples still mostly use Unix single quotes because that is the
  existing repository convention. Issue #18 and PR #49 track quote unification.
- The documentation describes current merged behavior. Several open PRs propose
  API/server features that are intentionally listed as open, not implemented.
- The Rust CLI does not implement persistent transformation trigger options yet;
  the docs call that out explicitly.
