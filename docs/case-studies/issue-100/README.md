# Issue 100 Case Study: Dependency Refresh, Upstream Reuse and Cross-Language Parity

Issue: <https://github.com/link-foundation/link-cli/issues/100>

Prepared PR: [#101](https://github.com/link-foundation/link-cli/pull/101)

> Scope of this case study: this folder captures the restated requirements, the
> evidence, the root-cause analysis of every behaviour gap found, the
> implemented design, and the verification evidence for updating every
> dependency in every language, leaning on the new `doublets` 0.5.0 features so
> less is duplicated here, opening every layer up for extension, and proving
> that the languages we support actually have the same features.

## 1. Issue summary

The issue is six sentences, each of which is a separate ask:

| # | Ask (verbatim) |
|---|----------------|
| 1 | "All remaining issues or missing features, that are good to have in doublets-rs should be reported there." |
| 2 | "We update exactly all dependencies in all languages, not just doublets-rs." |
| 3 | "But we should focus on latest release of doublets to reuse much of new features, so less code is duplicated in this repository." |
| 4 | "We also must double check that in all languages we provide all abstractions with all trust for extension, as much public members as possible, and so on. So everything is easy to reconfigure, reuse, swap and so on." |
| 5 | "So all programming languages we support should provide not only CLI (and other surfaces), but also a library itself to simplify alternative/custom CLIs construction and much more." |
| 6 | "We also must double check that all programming languages we support have all the same features, nothing is missing in any of languages." |

There are no comments on the issue; nothing narrowed or widened it after it
was filed.

Ask 6 is the one that generated most of the work. "Nothing is missing in any
of languages" cannot be answered by reading code — the two implementations
have different stores underneath them — so it was answered by *running* both
CLIs over the same query sequences and diffing the resulting databases. That
harness is [`evidence/cli-parity/run.sh`](evidence/cli-parity/run.sh), and it
found five real divergences that this PR fixes and one that belongs upstream.

## 2. Restated requirements

| ID  | Requirement |
|-----|-------------|
| R1  | Every gap in `doublets-rs` that this repository has to work around is reported upstream, with a reproduction. |
| R2  | Every dependency in Rust, C# and JS is at its newest published stable version, or the reason it is not is recorded. |
| R3  | `doublets` is on its latest release and its new capabilities are *used*, replacing code duplicated here. |
| R4  | Every layer in every language is open for extension: unsealed/public types, overridable members, and a seam a replacement can be slotted into. |
| R5  | Every language ships a library, not only a CLI, so an alternative CLI can be built from the same pieces. |
| R6  | The languages have the same features: same flags, same query semantics, same observable database after the same queries. |
| R7  | Any remaining cross-language difference is either fixed here, or attributed to a dependency defect with a reproduction and an upstream issue. |
| R8  | The CLI's existing observable behaviour is not regressed: same output for existing invocations, no new files. |

## 3. Evidence captured in this folder

```
docs/case-studies/issue-100/
├── README.md                        # This document.
└── evidence/
    ├── cli-parity/run.sh            # 39 scenarios run through both CLIs and diffed (§6).
    ├── csharp-merge-usages/         # Reproduction of the Data.Doublets MergeUsages defect (§5.5).
    │   ├── Program.cs
    │   ├── csharp-merge-usages.csproj
    │   └── run.sh
    └── external-range/              # Side-by-side of the C# and Rust LinksConstants ranges (§5.6).
        ├── csharp/{Program.cs,external-range.csproj,run.sh}
        └── rust/{Cargo.toml,Cargo.lock,src/main.rs,run.sh}
```

Both upstream reproductions follow the same convention: **`run.sh` exits 0
while the defect reproduces and non-zero once it is fixed**, so the day the
upstream release lands, the harness tells us instead of the defect quietly
outliving its workaround. `cli-parity/run.sh` applies the same rule to its one
`known_difference` scenario: if the two languages ever agree there, the
scenario turns *red* so the exemption gets removed.

## 4. Reporting upstream (R1)

Four defects and gaps were confirmed with runnable reproductions and filed:

| Issue | What |
|-------|------|
| [doublets-rs#60](https://github.com/linksplatform/doublets-rs/issues/60) | No Rust counterpart for `Platform.Data.Doublets.Sequences` — sequences, Unicode strings, walkers. This repository ports the Unicode string path by hand (`rust/src/unicode_string_storage.rs`, `rust/src/sequences.rs`); that port is exactly the duplication ask 3 asks us to remove, and it cannot be removed until upstream has the layer. |
| [doublets-rs#61](https://github.com/linksplatform/doublets-rs/issues/61) | No transactions layer: C# has `UInt64LinksTransactionsLayer`, Rust has nothing. `link-cli` maintains its own (`rust/src/transactions/`) for the same reason. |
| [Data.Doublets#515](https://github.com/linksplatform/Data.Doublets/issues/515) | `MergeUsages` writes null targets and wrong sources — the root cause of the one remaining C#/Rust divergence (§5.5). |
| [data-rs#18](https://github.com/linksplatform/data-rs/issues/18) | `LinksConstants::external()` overlaps the external range with the `continue` constant, so `is_external(continue)` is `true` in Rust and `False` in C# (§5.6). |

A fifth candidate was investigated and **not** filed: the address allocator.
`doublets` 0.5.0's `src/mem/unit/store.rs` already implements the exact
contract this repository reverse-engineered from C# — `UnusedLinks` plus
`header.first_free`, `attach_as_first` for LIFO reuse, and a tail shrink on
delete. There was nothing to ask for; the bug was on our side (§5.2).

## 5. Root-cause analysis

### 5.1 `doublets` 0.5.0 brings the decorator layer into reach

The gap ask 3 names is real: before this PR the Rust `LinkStorage` resolved
uniqueness and cascading deletes with its own code, while C# gets both from
`ILinksExtensions.DecorateWithAutomaticUniquenessAndUsagesResolution`.
`doublets` 0.5.0 exposes `doublets::decorators`, which is the same stack.

`DoubletsStorage::map_store` composes any upstream (or caller-written)
decorator onto an open database while keeping its path, its advisory lock and
its change-detection fingerprint, and
`DoubletsStorage::with_automatic_uniqueness_and_usages_resolution` applies the
C# stack by name. The `doublets` crate — including `decorators` — is
re-exported from `link_cli`, so a downstream crate can build its own stack
without adding a direct dependency that could drift to an incompatible semver.

Routing the CLI's `LinkStorage` through those resolvers is what made a Rust
delete cascade into the links that referenced the deleted one, and an update
that would duplicate an existing link merge into it, the way C# already did.

### 5.2 The stores handed out addresses in different orders

Which address a new link gets is *observable* — it is printed by `--after` and
it is what a later query refers to — so the two stores must allocate
identically or every subsequent query diverges.

C#'s store reuses a freed address before growing, most recently freed first,
shrinks when the last link is deleted, and gives back the addresses it passed
over while reaching a requested one. The Rust `LinkStorage` grew monotonically.
Six harness scenarios (`reuse a freed address`, `reuse after a shrink`,
`reuse the newest hole first`, `auto-create frees the addresses it passed
over`, `auto-create leaves the new link the first address`) pin the contract
down; the free list is now persisted so the order survives between CLI
invocations, because a CLI process is one query long.

### 5.3 `--changes` reported different changes

Three separate causes, all fixed:

- An auto-created reference was reported as a *creation* in Rust and as an
  *update of the placeholder it started from* in C#.
- A delete reported only the deleted link in Rust; C# reports the whole
  cascade of removed usages.
- The reported order came from `HashMap` iteration, so it varied with the
  process's hash seed. It is now deterministic.

### 5.4 Unspecified substitution halves were written literally

This is the divergence found last, and the subtlest. A substitution half that
no restriction ever bound — a never-bound variable, or a `*` — is
*unspecified*, not an address.

C# marks it with `links.Constants.Any`, which is a value the *store*
understands, and the store then gives it three meanings depending on where it
lands:

| Position | Meaning |
|----------|---------|
| `SearchOrDefault` | wildcard — the lookup runs through `Each`, which reads `any` as "every value" |
| an `Update` substitution | keep the half already stored |
| a create | null (`0`) |

which is one rule: *unspecified → the existing value, or null when there is
none.*

The Rust processor marked the same thing with `u32::MAX`, a value its store
does **not** recognise — the store's `any` is `2147483644`, the hybrid-aware
constant. So `() (($a $a))` stored the literal `4294967295` in both halves
where C# stores `(1: 0 0)`. Five of six probe shapes diverged.

The fix resolves at the **write boundary** rather than changing the sentinel:
`QueryProcessor::resolve_unspecified` and `QueryProcessor::search_unspecified`
sit in front of the five places the processor writes to or searches the store.
That keeps `u32::MAX` as the crate's single internal marker and leaves
restriction matching — and `NamedTypeLinks::search`, which is deliberately
literal because it backs uniqueness resolution — untouched.

### 5.5 `MergeUsages` in Platform.Data.Doublets 0.18.1 (upstream, not fixed here)

One scenario still diverges, and it is a C# bug, not a Rust one:

```
'() ((1 2) (2 1))'   '((1: 1 2)) ((1: 2 1))'
```

C# leaves `(2: 2 0)`; Rust leaves `(2: 2 2)`.

`Platform.Data.Doublets.Link<T>` declares `(params T[] values)`,
`(IList<T>)`, `(object)`, `(ref Link<T>)` and `(index, source, target)`. It
has **no** two-argument `(source, target)` constructor, so `new Link<uint>(a, b)`
binds to the `params` overload, and `SetValues` reads a two-element list as
`(index, source)` with `target = default`. `MergeUsages` constructs its
replacement links that way, so it repoints usages onto a link with a null
target and a source that is really the index.

[`evidence/csharp-merge-usages/run.sh`](evidence/csharp-merge-usages/run.sh)
reproduces this against `Platform.Data.Doublets` directly, with no `link-cli`
code involved. Filed as
[Data.Doublets#515](https://github.com/linksplatform/Data.Doublets/issues/515)
and recorded in the parity harness as its single `known_difference`.

### 5.6 The constants differ, and it is unreachable in practice

`doublets` 0.5.0 re-exports `platform-data` 2.0.0's `LinksConstants`.
`full_new` reserves six service values at the top of the internal range and
then takes the external range verbatim, so with external references enabled
`external_range` *starts on* `r#continue` — the two overlap by one address.
The C# `LinksConstants<TLinkAddress>` starts the external range one past the
half, so no service constant is ever reported as external.

The visible consequence for `link-cli` is that the main database's `any`
differs: `4294967292` in C# (default, internal-only constants) versus
`2147483644` in Rust (`LinksConstants::external()`, which every `LinkStorage`
reports because the hybrid external-reference half is not optional here — see
`rust/src/link_storage_doublets.rs`).

Reaching that difference through the CLI means naming address `2147483644` or
`4294967292` in a query. Both CLIs then try to allocate roughly two billion
links and neither finishes; the difference is theoretical rather than
observable at the CLI. It is documented here and filed as
[data-rs#18](https://github.com/linksplatform/data-rs/issues/18) rather than
worked around.

## 6. Implemented solution

### Rust

| Area | Change |
|------|--------|
| Basis | `doublets` 0.4.0 → 0.5.0 in both the `link-cli` and the `clink-wasm` lockfiles. |
| Reuse (R3) | `DoubletsStorage::map_store` and `::with_automatic_uniqueness_and_usages_resolution`; `doublets` and `doublets::decorators` re-exported from `link_cli`. |
| Cascades (R6) | `LinkStorage` routed through the upstream uniqueness and cascade resolvers; the transactions log records one transition per link a write actually touched, so rollback and branch switching no longer lose cascaded changes; the query processor restores links a resolved write deleted as a side effect, mirroring `RestoreUnexpectedLinkDeletions`. |
| Constants (R7) | `LinkStorage` reports the hybrid `LinksConstants`; its inherent `get_or_create` no longer resolves through the `Doublets` impl for `&mut LinkStorage`, which treats `any` as a wildcard. Name holders are ordered by address, so reserved pinned type names resolve deterministically. |
| Triggers (R6) | `PersistentTransformationDecorator` ports the C# persistent transformation triggers — `Once`/`Always` schema, the `<database>.triggers.links` sidecar, and the embedded store. Triggers written by either implementation are readable by the other. Exposed on the CLI as `--always`, `--once`, `--never`, `--triggers`, `--triggers-file`, `--embed-triggers`. |
| Addresses (R6) | Address allocation matches the C# store, free list persisted (§5.2). |
| `--changes` (R6) | Placeholder updates, full delete cascades, deterministic order (§5.3). |
| Unspecified halves (R6) | `resolve_unspecified` / `search_unspecified` at the write boundary (§5.4). |
| Layout | The two files the new code pushed past the 1000-line CI gate are split the way the repository already splits large ones — a child module reaching the parent's private items, so nothing is widened beyond `pub(super)`: `query_processor/mutations.rs` (the write side, next to the existing `query_processor/matching.rs`, mirroring C#'s `AdvancedMixedQueryProcessor.Mutations.cs`) and `transactions/recovery.rs` (transition replay, crash recovery, log retention). |
| Library (R4, R5) | Every module of `link_cli` is public, along with the query patterns, the resolved links, the link reference validator and the transition wire-format constants. `NamedTypeLinks` is documented as *the* seam: every layer is written against it and every decorator both implements it and wraps another implementation of it, so a cache, an access check or a remote store slots in anywhere — including under `QueryProcessor`, which never learns what is beneath it. |

### C#

| Area | Change |
|------|--------|
| Extension (R4) | Every decorator — `NamedTypesDecorator`, `NamedLinksDecorator`, `SimpleLinksDecorator`, `PinnedTypesDecorator`, `TransactionsDecorator`, `VersionControlDecorator`, `PersistentTransformationDecorator` — is unsealed with overridable members. The disposable ones follow `protected virtual void Dispose(bool)` so a subclass can release resources of its own. |
| Publication (R4) | `PersistentTransformationDecorator.PersistentTransformationQuery` and `InternalNamePrefix` are public, matching what the Rust library already exposed. |
| Enforcement | `ExtensibilityTests` subclasses four of the decorators and asserts the seam reflectively, so re-sealing a class or dropping a `virtual` fails the suite rather than silently narrowing the API. |

The C# library was already a separate project
(`Foundation.Data.Doublets.Cli.Library`) that `Foundation.Data.Doublets.Cli`
consumes, so R5 was already satisfied there; what was missing was R4, which is
what this change delivers.

### JS

The JS surface is a web front end over the Rust WASM build, not a third
implementation of the CLI, so R6 does not apply to it as a feature matrix —
it inherits whatever `clink-wasm` exposes. Its dependencies were checked and
are current (§7).

### Documentation

`docs/` no longer claims persistent transformation triggers are C#-only; the
Rust CLI has them as of this PR.

## 7. Dependencies (R2)

Every dependency in every language was checked against its registry. Only one
was behind:

| Language | Dependency | Before | After |
|----------|-----------|--------|-------|
| Rust | `doublets` | 0.4.0 | **0.5.0** |

Everything else was already at its newest published stable version and is
recorded here so the check is auditable rather than implied:

| Language | Dependency | Version |
|----------|-----------|---------|
| Rust | `thiserror` | 2.0.20 |
| Rust | `anyhow` | 1.0.104 |
| Rust | `links-notation` | 0.16.1 |
| Rust | `lino-arguments` | 0.3.0 |
| Rust (WASM) | `wasm-bindgen` / `wasm-bindgen-test` | 0.2.127 / 0.3.77 |
| Rust (WASM) | `serde` / `serde_json` | 1.0.229 / 1.0.151 |
| Rust (WASM) | `web-sys` | 0.3.104 |
| Rust (WASM) | `console_error_panic_hook` | 0.1.7 |
| C# | `Link.Foundation.Links.Notation` | 0.16.1 |
| C# | `Platform.Data` | 0.16.1 |
| C# | `Platform.Data.Doublets` | 0.18.1 |
| C# | `Platform.Data.Doublets.Sequences` | 0.6.5 |
| C# | `System.CommandLine` | 2.0.11 |
| C# | `xunit` / `xunit.runner.visualstudio` | 2.9.3 / 4.0.0 |
| C# | `Microsoft.NET.Test.Sdk` | 18.9.0 |
| C# | `coverlet.collector` | 10.0.1 |
| JS | `doublets-web` | ^0.1.3 |
| JS | `react` / `react-dom` | ^19.2.8 |
| JS | `lucide-react` | ^1.37.0 |
| JS | `vite` / `@vitejs/plugin-react` / `vite-plugin-wasm` | ^8.2.2 / ^6.1.1 / ^3.6.0 |

`System.CommandLine` deserves a note: 3.0.0 exists on NuGet but only as a
prerelease, so 2.0.11 is the newest *stable* release and the pin stays.

`links-notation` remains deliberately aligned at 0.16.1 across Rust and C#,
the invariant established in [issue 98](../issue-98/README.md#5-implemented-solution).

## 8. Verification

| Check | Result |
|-------|--------|
| `cargo fmt --all -- --check` (both workspaces) | clean |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo test` | 239 passed, 1 ignored |
| `dotnet format --verify-no-changes` | clean |
| `dotnet build --configuration Release` | 0 warnings, 0 errors |
| `dotnet test` | 254 passed |
| `node --test` (JS) | 9 passed |
| `evidence/cli-parity/run.sh` | 39 scenarios agree, 1 known upstream difference |

The runs behind those numbers are kept in
[`dev/log/issues/100/pulls/101/verification/`](../../../dev/log/issues/100/pulls/101/verification),
per the convention established in issue 96.

The parity harness is the primary evidence for R6. It runs the same query
sequence through both binaries, then compares two things rather than one:

- the **final database dump**, and
- one accepted/rejected verdict **per query**.

The verdicts are compared alongside the dumps because a query both CLIs refuse
leaves two empty databases, which a dump-only comparison would happily call a
match. The exit status is compared rather than the message text: the two
implementations are expected to agree on *what* they accept, not on how they
word a rejection. Trigger scenarios additionally dump the trigger sidecar, so
how a trigger is *stored* is compared and not just what it did.

Coverage: creates, duplicate creates, updates, deletes, cascade deletes,
cascade chains, uniqueness-on-update, structural and wildcard deletes, named
links and named cascades, renames, nested composites, explicit indexes after a
gap, reverse update chains, self-referencing deletes, the six
address-allocation scenarios, the six unspecified-half scenarios, and eight
trigger scenarios including the embedded store.

## 9. What the issue asked for versus what shipped

| Ask | Status |
|-----|--------|
| 1. Report gaps to doublets-rs | Done: [doublets-rs#60](https://github.com/linksplatform/doublets-rs/issues/60), [#61](https://github.com/linksplatform/doublets-rs/issues/61), plus [Data.Doublets#515](https://github.com/linksplatform/Data.Doublets/issues/515) and [data-rs#18](https://github.com/linksplatform/data-rs/issues/18) in the neighbouring repositories the defects actually live in. The allocator was investigated and found already correct upstream, so nothing was filed for it (§4). |
| 2. All dependencies, all languages | Done and audited. One was behind (`doublets` 0.4.0 → 0.5.0); the other 22 are recorded at their current versions in §7, with `System.CommandLine` explicitly noted as latest-*stable*. |
| 3. Reuse the latest doublets, duplicate less | Done: uniqueness and cascade resolution now come from `doublets::decorators` instead of hand-written code, and `map_store` makes the whole upstream decorator layer composable. The duplication that *remains* — the Unicode/sequences port and the transactions layer — has no upstream counterpart yet, which is why it is filed as doublets-rs#60 and #61 rather than removed. |
| 4. Everything open for extension | Done in both languages. Rust: every module public plus the query and validation types; `NamedTypeLinks` documented as the swap-in seam. C#: every decorator unsealed and overridable, `Dispose(bool)` pattern, trigger query and name prefix published — enforced by reflective tests so it cannot silently regress. |
| 5. A library, not only a CLI | Already true in C#; now true in Rust down to the last module. The JS surface is a front end over the WASM build rather than a third implementation. |
| 6. Same features everywhere | Done, and *proved* rather than asserted: 39 scenarios agree across both CLIs. Five divergences were found and fixed here (cascades, addresses, constants, `--changes`, unspecified halves) and the Rust CLI gained the trigger flags it was missing. |
| 7. Remaining differences attributed | One remains, and it is an upstream C# defect with a standalone reproduction and an upstream issue (§5.5). |

## 10. Risks and follow-ups

- **The `MergeUsages` exemption.** Until
  [Data.Doublets#515](https://github.com/linksplatform/Data.Doublets/issues/515)
  is released, `update into duplicate` produces a corrupt link in C#. The
  harness will turn red the moment the languages agree, which is the signal to
  drop the exemption.
- **The constants overlap.** `is_external(continue)` is `true` in Rust and
  `False` in C#. Unreachable through the CLI (§5.6), but a *library* consumer
  that calls it directly will see the difference. Tracked as
  [data-rs#18](https://github.com/linksplatform/data-rs/issues/18).
- **Duplicated layers.** The Unicode/sequences port and the transactions layer
  are still maintained here because Rust has no upstream equivalent. They are
  the standing answer to ask 3 and should be deleted in favour of upstream once
  doublets-rs#60 and #61 land.
- **`u32::MAX` as an internal marker.** The unspecified-half fix resolves at
  the write boundary rather than adopting the store's `any`. That is
  deliberate — it keeps one marker and leaves restriction matching alone — but
  it means a *new* write path added to the query processor has to route through
  `resolve_unspecified`/`search_unspecified` to stay correct. The six parity
  scenarios and six unit tests are the guard.
- **Advisory locking and MSRV 1.89** carry over unchanged from
  [issue 98](../issue-98/README.md#8-risks-and-follow-ups).
