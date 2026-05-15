---
'Foundation.Data.Doublets.Cli': minor
---

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
