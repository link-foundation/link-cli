# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).




## [3.0.0] - 2026-08-29

Refreshed every C# dependency to its latest release and retargeted the
packages to `net10.0` (issue #98). `Link.Foundation.Links.Notation`
moves 0.13.0 -> 0.16.1 so the C# and Rust implementations parse LiNo
with the same version of the same grammar; that release only ships a
`net10.0` assembly, so `Directory.Build.props` and all three projects
now target `net10.0` and CI provisions the .NET 10 SDK. `System.CommandLine`
moves 2.0.7 -> 2.0.11, and the test project picks up
`Microsoft.NET.Test.Sdk` 18.9.0, `xunit.runner.visualstudio` 4.0.0 and
`coverlet.collector` 10.0.1.

This is a breaking change for consumers still on `net8.0`: upgrade to
the .NET 10 SDK/runtime before taking this release.

Made the C# transactions layer reusable from outside the CLI, matching
what the Rust library now offers (issue #98).

`TransactionsDecorator` is generic over the doublets address type:
`TransactionsDecorator<TLinkAddress>` works over any
`INamedTypesLinks<TLinkAddress>` whose address is an
`IUnsignedNumber<TLinkAddress>`, so a consumer with a `ulong`-addressed
store is a first-class user rather than being pinned to `uint`. The
non-generic `TransactionsDecorator` remains as a `uint` specialisation,
so existing code that constructs it keeps compiling. The transitions
wire format is unchanged and address-type independent — addresses are
written in decimal under the invariant culture, so a log written by a
`uint`-addressed store reads back unchanged in a `ulong`-addressed one,
and an address that does not fit the target type is rejected instead of
being silently truncated.

New `LinksFileLock` and `StorageRevision` cover multi-process access:
advisory locking of a database's `.lock` sidecar (shared for readers,
exclusive for writers, with a blocking `Acquire` and a non-blocking
`TryAcquire`) and a cheap "has anyone else written since I last looked?"
fingerprint. The lock file path and the shared/exclusive semantics match
the Rust `storage::lock` module, so the two implementations can guard the
same database.

Source-breaking: `Transition`, `ITransaction` and `ITransactionsLinks`
are now generic. Existing `uint` code should use `Transition<uint>`,
`ITransaction<uint>` and `ITransactionsLinks<uint>`.

## [2.6.0] - 2026-08-18

Added optional transactions and version-control layers (issue #94). The
new `TransactionsDecorator` records each Create/Update/Delete as a
reversible transition in a sidecar doublets store and exposes
`BeginTransaction()` / `Commit()` / `Rollback()` plus three retention
policies (`infinite`, `sized:<n>`, `chunked:<n>:<dir>`) and two commit
modes (`sync`, `async`). The new `VersionControlDecorator` adds
branching, tagging, and time-travel checkout over that log. The CLI
surfaces both layers through `--transactions`, `--transactions-file`,
`--commit-mode`, `--retention`, `--log`, `--vc`, `--vc-file`,
`--branch`, `--branch-from`, `--checkout`, `--tag`, `--list-branches`,
and `--list-tags`. When no flag is passed, behaviour is byte-identical
to the existing CLI — no sidecar is written and no extra cost is paid.

Hardened the C# build and the pipelines around it (issue #96).
`Directory.Build.props` now turns warnings into errors and enables the
.NET analyzers, and `TransactionsDecorator` / `VersionControlDecorator`
implement `IDisposable` so the memory-mapped databases they own are
released deterministically — the leak that made the Windows test job
fail while the pipeline still reported success. The C# workflow no
longer masks those Windows failures with `continue-on-error`, verifies
formatting and file sizes, and finally implements the `changeset-pr`
release mode it had been advertising without handling.
Pull requests also re-run the build and tests on a simulated merge with
the tip of `main`, and the coverage upload moved to
`codecov/codecov-action@v7` to stop the Node.js 20 deprecation warning.

## [2.5.0] - 2026-05-15

Split the C# distribution into two NuGet packages so external .NET
projects can consume the public library without pulling in the
`dotnet tool` packaging:

- `clink` — unchanged dotnet tool, now built from a CLI csproj that only
  contains `Program.cs` and `System.CommandLine` wiring.
- `Foundation.Data.Doublets.Cli` — new library package that ships the
  parser, query processors (basic / advanced / mixed), `ChangesSimplifier`,
  named/pinned type decorators, persistent transformation trigger
  decorator, LiNo I/O adapters, the `UnicodeStringStorage` extension, and
  every other reusable building block. Generated XML doc comments are
  packed alongside the assembly and rendered into a DocFX site published
  to GitHub Pages.

## [2.4.0] - 2026-05-12

Added `--export` as an alias for `--out` database export.

Added `--in`/`--lino-input`/`--import` database import support for reading LiNo files into the links database with named references enabled by default.

Added `--out`/`--lino-output` database export support that writes the complete links database as LiNo with named references when available.

Added a universal `NamedTypesDecorator` that implements both links operations and named type lookups, with automatic cleanup and uniqueness checks for external-reference names.

Added binary links-backed persistent transformation triggers with `--always`, `--once`, `--never`, `--triggers-file`, and `--embed-triggers`.

Added `IPinnedTypes` and `PinnedTypesDecorator`, and composed pinned type support into `NamedTypesDecorator`.

Fixed self-link substitution with outgoing links by preserving unbound substitution parts from the matched link and rejecting unsupported link addresses during explicit creation.

Fixed explicit indexed numeric updates so auto-created numeric references do not steal the substitution pair, and added issue 62 regression coverage.

Moved C# release automation into `csharp/scripts/` and packaged the C# README
with the NuGet tool.

Added full string ID alias support for advanced LiNo queries through the named types decorator.

Updated the C# LiNo parser dependency to the current `Link.Foundation.Links.Notation` package and refreshed supported NuGet package versions.

Added strict validation for missing numeric and named link references, plus `--auto-create-missing-references` to create missing references as self-referential point links.
