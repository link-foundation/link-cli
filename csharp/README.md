# clink C# Package

[![C# CI/CD Pipeline](https://github.com/link-foundation/link-cli/actions/workflows/csharp.yml/badge.svg)](https://github.com/link-foundation/link-cli/actions/workflows/csharp.yml)
[![NuGet](https://img.shields.io/nuget/v/clink?logo=nuget&label=NuGet)](https://www.nuget.org/packages/clink)
[![GitHub Release](https://img.shields.io/github/v/release/link-foundation/link-cli?filter=csharp-v*&label=C%23%20release)](https://github.com/link-foundation/link-cli/releases)

This directory contains the production .NET CLI implementation published as
the NuGet tool package `clink`.

## Install

```bash
dotnet tool install --global clink
```

Update an existing installation:

```bash
dotnet tool update --global clink
```

## Use

```bash
clink '() ((1 1))' --changes --after
```

The C# package exposes the complete command surface, including persistent
transformation triggers with `--always`, `--once`, `--never`, `--triggers`,
`--triggers-file`, and `--embed-triggers`.

## Develop

```bash
dotnet restore
dotnet build --configuration Release
dotnet test --configuration Release
```

Release automation for this package lives in `csharp/scripts/` and uses
changesets from `csharp/.changeset/`.
