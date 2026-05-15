# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


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
