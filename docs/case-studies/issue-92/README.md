# Issue 92 Case Study: Ship public library packages alongside CLI for both C# and Rust

Issue: https://github.com/link-foundation/link-cli/issues/92

Prepared PR: [#93](https://github.com/link-foundation/link-cli/pull/93)

## Restated requirements

From the issue body and the linked templates, broken into testable requirements:

1. **R1 – C# NuGet must publish a public library**, not only a CLI tool. Any
   external .NET project must be able to add a `<PackageReference>` and reuse
   all the public APIs of `Foundation.Data.Doublets.Cli` (parser, query
   processors, decorators, storage adapters, named/pinned types, persistent
   transformation decorator, LiNo import/export).
2. **R2 – Rust Crates.io must publish a public library**, not only a CLI
   binary. Any external Rust project must be able to `cargo add link-cli` and
   reuse all the public APIs of `link_cli` (storage, parser, query processor,
   named types, LiNo I/O).
3. **R3 – C# CLI must remain available** as a .NET global tool published to
   NuGet (`dotnet tool install --global clink`).
4. **R4 – Rust CLI must remain available** as a binary published to Crates.io
   (`cargo install link-cli`).
5. **R5 – Both languages must ship the same feature surface in the library**
   that the CLI uses. The library should not be a thin subset.
6. **R6 – Automatically generated API documentation** must be published for
   both libraries.
   - C#: `<GenerateDocumentationFile>true</GenerateDocumentationFile>` plus
     a DocFX-built site hosted on GitHub Pages (matching the
     [csharp-ai-driven-development-pipeline-template](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template)).
   - Rust: docs.rs already builds rustdoc for published crates; on top of
     that, the rust template also deploys `cargo doc --no-deps --all-features`
     to GitHub Pages (`deploy-docs` job in
     [rust-ai-driven-development-pipeline-template/.github/workflows/release.yml](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/main/.github/workflows/release.yml)).
7. **R7 – Reuse CI/CD best practices** from the four templates
   (`csharp-`, `rust-`, `js-`, `python-ai-driven-development-pipeline-template`)
   and report any defects discovered in the templates themselves.
8. **R8 – Compile case-study data** to `./docs/case-studies/issue-92/`, list
   every requirement, propose solutions, and search online for additional
   facts.
9. **R9 – Plan and execute everything in a single pull request** (PR #93).

## Evidence captured in this folder

- `github-data/issue-92.json` and `github-data/issue-92-comments.json` — the
  upstream issue and its comments at investigation time.
- `github-data/pr-93.json` — PR snapshot.
- `templates/<template-name>/file-tree.txt` — full file listing of each of
  the four templates.
- `templates/csharp-ai-driven-development-pipeline-template/MyPackage.csproj.snapshot`
  — the template's library `.csproj` showing `<GenerateDocumentationFile>true</GenerateDocumentationFile>`
  and the absence of `<PackAsTool>true</PackAsTool>`. This is the structure
  the link-cli C# project should follow for its library half.
- `templates/csharp-ai-driven-development-pipeline-template/docfx.json.snapshot`
  and `docs.yml.snapshot` — DocFX configuration and the GitHub Pages workflow
  to deploy the resulting site.
- `templates/csharp-ai-driven-development-pipeline-template/Directory.Build.props.snapshot`
  — common build properties used by the template (strict warnings, analyzers,
  latest C# version).
- `templates/rust-ai-driven-development-pipeline-template/Cargo.toml.snapshot`
  — the template `Cargo.toml` defining both `[lib]` and `[[bin]]` and showing
  the `[lints.*]` blocks plus `[profile.release]` LTO/strip settings.
- `templates/rust-ai-driven-development-pipeline-template/lib.rs.snapshot` —
  the template's library entry point.
- `templates/rust-ai-driven-development-pipeline-template/release.yml.snapshot`
  — the full pipeline including the `deploy-docs` job (lines 636–675) that
  publishes `cargo doc --no-deps --all-features` to GitHub Pages.

## Online research

- Microsoft Learn — _".NET tools"_ documentation explains that a `dotnet tool`
  package is a NuGet package that wraps a console application. It is consumed
  via `dotnet tool install`, **not** via `PackageReference`. Source:
  <https://learn.microsoft.com/en-us/dotnet/core/tools/global-tools>.
- Microsoft Learn — _"Create a NuGet package using MSBuild"_ — a single
  `.csproj` produces one NuGet package per build. Packing both a tool and a
  library from the same code base therefore requires two projects (or two
  packs of the same project with different `<PackAsTool>` values). Source:
  <https://learn.microsoft.com/en-us/nuget/create-packages/creating-a-package-msbuild>.
- docs.rs `link-cli` page confirms that the Rust crate is already discoverable
  on docs.rs because it has a `[lib]` target; documentation coverage at
  investigation time was 19.19 %. Source: <https://docs.rs/link-cli>.
- docs.rs _"About / Builds"_ documents that every crate published to crates.io
  is built; bin-only crates without a `[lib]` produce no rustdoc. Source:
  <https://docs.rs/about/builds>.
- NuGet supports embedding XML documentation in a package via the SDK's
  `<GenerateDocumentationFile>true</GenerateDocumentationFile>` MSBuild
  property. Source: <https://learn.microsoft.com/en-us/dotnet/csharp/language-reference/xmldoc/>.
- DocFX is the de-facto static site generator for .NET XML doc comments and
  is what the csharp template uses. Source:
  <https://dotnet.github.io/docfx/>.

## Root cause

`csharp/Foundation.Data.Doublets.Cli/Foundation.Data.Doublets.Cli.csproj`
declared both `<OutputType>Exe</OutputType>` and `<PackAsTool>true</PackAsTool>`
on a **single project**:

```xml
<OutputType>Exe</OutputType>
<TargetFramework>net8</TargetFramework>
<PackAsTool>true</PackAsTool>
<ToolCommandName>clink</ToolCommandName>
<PackageId>clink</PackageId>
```

As a .NET tool, the resulting NuGet package can only be installed with
`dotnet tool install --global clink`. .NET tools intentionally cannot be
consumed by other projects via `<PackageReference>` — the package layout
under `tools/<tfm>/any/` is not the layout `restore` resolves to when adding
a library dependency. The implication is that:

- All the source code (parser, query processors, named-type decorator,
  pinned-type decorator, persistent transformation decorator, LiNo I/O,
  Unicode string storage) is already **`public`** in the assembly, but
- It is shipped inside a tool-only NuGet package, so downstream .NET code
  cannot reuse it.

The Rust side does not have this defect because `rust/Cargo.toml` already
declares both `[lib] name = "link_cli"` and `[[bin]] name = "clink"`. The
crate is therefore consumed in three ways already:

- `cargo install link-cli` → installs the `clink` binary;
- `cargo add link-cli` → adds the `link_cli` library;
- `https://docs.rs/link-cli` → renders rustdoc automatically.

What is missing on the Rust side is:

- The `deploy-docs` workflow job that the rust template publishes to GitHub
  Pages — useful as a single homepage for the project's API docs.
- Any explicit "this crate is also a library" callout in `rust/README.md`.

## Solution

### S1 — Split the C# project into a library and a tool

Refactor `csharp/`:

```
csharp/
├── Foundation.Data.Doublets.Cli.Library/        # NEW: library project
│   ├── Foundation.Data.Doublets.Cli.Library.csproj
│   └── *.cs                                     # moved from CLI project
├── Foundation.Data.Doublets.Cli/                # CLI tool project (unchanged id `clink`)
│   ├── Foundation.Data.Doublets.Cli.csproj      # now references Library, contains only Program.cs
│   └── Program.cs
└── Foundation.Data.Doublets.Cli.Tests/          # references Library
```

- Library `.csproj`: regular SDK library, no `<PackAsTool>`, with
  `<PackageId>Foundation.Data.Doublets.Cli</PackageId>`,
  `<GenerateDocumentationFile>true</GenerateDocumentationFile>`, and explicit
  package metadata mirroring the template.
- CLI `.csproj`: keeps `<PackAsTool>true</PackAsTool>`,
  `<ToolCommandName>clink</ToolCommandName>`,
  `<PackageId>clink</PackageId>`, and adds a `<ProjectReference>` to the
  library plus `<PackageReference>` for the library NuGet so the resolved
  tool package depends on the published library at the same version. The
  tool's source set shrinks to `Program.cs`.
- All existing namespaces (`Foundation.Data.Doublets.Cli`) stay the same so
  the test project (and any downstream code) keeps compiling.
- `Foundation.Data.Doublets.Cli.sln` adds the new library project.

### S2 — Pack and publish both NuGet packages

Update `.github/workflows/csharp.yml` so that both the `release` and
`instant-release` jobs:

- run `dotnet pack` for **all** packable projects in the solution
  (i.e. drop the implicit single-project assumption);
- run `dotnet nuget push ./artifacts/*.nupkg ... --skip-duplicate` so that
  both the library and the tool get pushed;
- still validate that the library appears on NuGet via the existing
  `wait-for-nuget.mjs` script (now parameterized over both ids).

`csharp/scripts/check-release-needed.mjs` already reads
`<Version>` from `Foundation.Data.Doublets.Cli.csproj`; the library project
mirrors that version so both packages share a release cycle.

### S3 — Generate and host C# API docs

- Add `docs/case-studies/issue-92/...` (this case study) plus
  `csharp/docfx.json` configured to pick up the library project.
- Add `.github/workflows/csharp-docs.yml`, structurally identical to the
  csharp template's `docs.yml`, gated to `main` and manual dispatch only,
  building with `docfx csharp/docfx.json -o csharp/_site` and deploying via
  `actions/deploy-pages@v5`.
- Set `<GenerateDocumentationFile>true</GenerateDocumentationFile>` on the
  library project so each NuGet build embeds the XML docs.

### S4 — Add Rust API docs deploy + tighten lib metadata

- Surface that the crate is also a library in `rust/README.md`.
- Tighten `rust/Cargo.toml`:
  - Add `documentation = "https://docs.rs/link-cli"`.
  - Add `categories = ["command-line-utilities", "database", "data-structures"]`.
- Add the `deploy-docs` job to `.github/workflows/rust.yml`, gated to
  `push to main` / `workflow_dispatch`, that runs
  `cargo doc --no-deps --all-features` from `rust/` and publishes
  `rust/target/doc` via `actions/upload-pages-artifact@v5` +
  `actions/deploy-pages@v5`. Mirrors the rust template's job (lines 636–675).

### S5 — README and examples

- Update root `README.md` to list both NuGet packages and a one-liner
  example for each (CLI install vs. library reference).
- Update `csharp/README.md` and `rust/README.md` to document the library
  surface and link to the generated docs.
- Add `examples/library-csharp/` and `examples/library-rust/` minimal sample
  projects so contributors can validate the library experience locally.

### S6 — Tests for the library surface

- Existing xUnit tests already use the public APIs; they automatically cover
  the library after the project split.
- Add an integration test that builds the library `.csproj` and a tiny
  consumer `.csproj` to assert that referencing the produced `.nupkg` works
  end to end (rather than the implicit `ProjectReference`).
- Existing Rust integration tests in `rust/tests/` already exercise the
  `link_cli::*` library surface and remain unchanged.

## Template defects to flag (R7)

While comparing, no defect was found that prevents the templates from
generating both a library and a CLI — the csharp template ships **only** a
library and the rust template ships **only** a library (no `[[bin]]`).
Neither template demonstrates the joint case directly. This is not a defect
per se, but it explains why the link-cli C# project diverged: the template
did not show the dual-project pattern.

Follow-up suggestion (filed as an upstream improvement in the link-cli PR
description rather than a separate issue, per the case-study convention):
extend the csharp template with a second project that demonstrates packing
the library as both a regular NuGet and a `PackAsTool` CLI sharing the same
library. If the link-foundation maintainers agree, file
`link-foundation/csharp-ai-driven-development-pipeline-template#new` once
this PR ships.

## Verification plan

- `dotnet build csharp/Foundation.Data.Doublets.Cli.sln -c Release` succeeds
  with `TreatWarningsAsErrors=true` once XML doc warnings are silenced
  (`CS1591` allow-list, per template).
- `dotnet test csharp/Foundation.Data.Doublets.Cli.sln -c Release` keeps
  passing without test changes.
- `dotnet pack csharp/Foundation.Data.Doublets.Cli.Library/...csproj`
  produces a `Foundation.Data.Doublets.Cli.<version>.nupkg` whose
  `lib/net8.0/` folder contains the assembly and `*.xml` doc file.
- `dotnet pack csharp/Foundation.Data.Doublets.Cli/...csproj` produces a
  `clink.<version>.nupkg` with `tools/net8.0/any/` content.
- `cargo build --manifest-path rust/Cargo.toml --release` keeps working.
- `cargo doc --no-deps --all-features --manifest-path rust/Cargo.toml`
  produces `rust/target/doc/link_cli/index.html`.
- CI on PR #93 keeps passing across `ubuntu-latest`, `macos-latest`,
  `windows-latest`.
