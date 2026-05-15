# Foundation.Data.Doublets.Cli

The `Foundation.Data.Doublets.Cli` NuGet package exposes the parser, query
processors (basic, advanced, mixed), `ChangesSimplifier`, the named/pinned
type decorators, the persistent transformation trigger decorator, LiNo
import/export adapters, the `UnicodeStringStorage` extension, and every
other building block the [`clink`](https://www.nuget.org/packages/clink)
.NET tool is composed of. External .NET projects can pull it in via
`<PackageReference>` and recreate or extend the CLI behavior without
re-implementing any of the underlying machinery.

The companion `clink` package keeps shipping as a `dotnet tool` so users
can install the CLI with `dotnet tool install --global clink`.

Browse the [API reference](api/Foundation.Data.Doublets.Cli.yml) for the
full list of namespaces and types.
