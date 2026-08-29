---
'Foundation.Data.Doublets.Cli': major
---

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
