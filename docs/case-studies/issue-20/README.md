# Issue 20 Case Study: Self-Link Substitution with Outgoing Link

Issue: https://github.com/link-foundation/link-cli/issues/20
Pull request: https://github.com/link-foundation/link-cli/pull/47

## Evidence Collected

- `evidence/issue-20.json`: original issue details and reproduction commands.
- `evidence/issue-20-comments.json`: issue comments.
- `evidence/pr-47.json`: PR state before this update.
- `evidence/pr-47-conversation-comments.json`: PR feedback requesting a merge
  from `main`, removal of iteration limits, C#/Rust parity tests, and this case
  study.
- `evidence/pr-47-review-comments.json` and `evidence/pr-47-reviews.json`:
  inline review and review-state payloads.
- `evidence/issue-20-screenshot.png`: original screenshot. The local `file`
  command was unavailable, so the PNG signature was verified with `od`:
  `89 50 4e 47 0d 0a 1a 0a`.
- `evidence/linksplatform-data-point.cs`: upstream `Point<T>` implementation
  containing `IsFullPoint` and `IsPartialPoint`.
- `evidence/linksplatform-data-ilinksextensions.cs`: upstream data extension
  functions, including internal-reference and point helpers.
- `evidence/linksplatform-data-doublets-ilinksextensions.cs`: upstream
  `FormatStructure` implementation that uses a visited set for recursive link
  formatting.
- `evidence/linksplatform-data-linksconstants.cs`: upstream link constants and
  internal-reference range behavior.
- `evidence/repro-after-fix.log`: traced local reproduction after the fix.
- `evidence/dotnet-*.log`, `evidence/cargo-*.log`, `evidence/npm-*.log`,
  and `evidence/diff-check.log`: final local verification logs.
- `evidence/ci-rust-lint-75003226910.log`: CI log from the first post-push
  attempt showing the Rust file-size gate failure after `query_processor.rs`
  exceeded 1000 lines.
- `evidence/rust-file-size-check.log`: final local Rust file-size check after
  moving substitution-preservation helpers into a separate module.

## Timeline

- Issue 20 reported that this sequence exhausted memory:
  `clink '() ((21: 21 21))'`, then
  `clink '((($i: 1 21)) (($i: $s $t) ($i 20)))'`.
- The first PR draft added iteration limits in `LinksExtensions.EnsureCreated`.
- PR feedback rejected iteration limits and requested deterministic validation,
  upstream LinksPlatform point/structure patterns, Rust parity, and case-study
  artifacts.
- This branch was merged with current `main`, adopting the `csharp/` layout and
  the Rust implementation added on `main`.

## Root Causes

- `ApplySolutionToPattern` resolved unbound substitution variables as the
  special `Any` address. For `($i: $s $t)`, `$i` was bound but `$s` and `$t`
  became `Any`, so updates could feed special constants into link creation.
- Anonymous substitution composites such as `($i 20)` were also resolved with
  the special `Any` index instead of the null index used for "create a new
  link".
- `EnsureCreated` clamped unsupported addresses to the maximum internal range
  and then repeatedly called the creator until that maximum appeared. If a
  special constant or recursive substitution reached this path, the loop could
  allocate until the process ran out of memory.

## Solution Applied

- Indexed substitution patterns now preserve unbound source/target variables
  from the existing matched link. This keeps `($i: $s $t)` as the matched link's
  current structure when only `$i` was bound by the restriction.
- Anonymous substitution composites now keep index `0`, so `($i 20)` creates a
  new outgoing link instead of going through wildcard substitution handling.
- `EnsureCreated` now rejects null, special, and out-of-range addresses using
  the supported internal-reference range. It also detects repeated creator
  output with a visited set and rejects overshooting the requested target,
  without arbitrary iteration limits.
- Rust parity now follows the same substitution-preservation rule and uses a
  checked creation path before explicit-ID creation.
- Rust substitution-preservation helpers were moved out of
  `query_processor.rs` after CI showed the repository's 1000-line file-size
  gate was exceeded.
- The C# preservation helper uses the upstream point helpers and a visited set
  so direct self-points and partial points are preserved without recursive
  expansion.

## Verification

- C# targeted tests:
  `dotnet test csharp/Foundation.Data.Doublets.Cli.Tests/Foundation.Data.Doublets.Cli.Tests.csproj --filter "Issue20|EnsureCreated_WithSpecialAnyReference"`
- C# full checks:
  `dotnet restore`, `dotnet build --no-restore --configuration Release`, and
  `dotnet test --no-build --configuration Release --verbosity normal` from
  `csharp/`.
- Rust targeted tests:
  `cargo test --manifest-path rust/Cargo.toml issue_20`
- Rust full checks:
  `cargo fmt --all -- --check`, `RUSTFLAGS=-Dwarnings cargo clippy --all-targets --all-features`,
  `cargo test --all-features --verbose`, and `cargo test --doc --verbose`
  from `rust/`.
- Rust file-size check:
  `node scripts/check-file-size.mjs --lang rust`.
- WebAssembly checks after touching `rust/`:
  `npm ci`, `npm run test:wasm`, and `npm run build`.
- Whitespace check:
  `git diff --check`.
- Manual traced reproduction after the fix creates `(2: 18 20)` while keeping
  `(18: 1 21)` and `(21: 21 21)` intact. See
  `evidence/repro-after-fix.log`.

## Residual Risks

- The query processor still has older broad wildcard handling paths. The new
  tests cover the issue-20 direct and full-point cases, but more complex nested
  substitution aliases should continue to get parity tests as behavior is
  specified.
