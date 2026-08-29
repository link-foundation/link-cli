#!/usr/bin/env bash
# Prints the C# reference constants the Rust overlap is measured against.
set -u
cd "$(dirname "$0")"
dotnet run -v q --nologo
