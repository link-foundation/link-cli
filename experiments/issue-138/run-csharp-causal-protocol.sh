#!/usr/bin/env bash
# Focused protocol checks only. Full suites/coverage run in the C# CI matrix.
set -euo pipefail
cd "$(dirname "$0")/../.."
export DOTNET_PROCESSOR_COUNT=2 DOTNET_GCHeapHardLimit=0x40000000
export MSBUILDDISABLENODEREUSE=1 DOTNET_CLI_TELEMETRY_OPTOUT=1
dotnet test csharp/Foundation.Data.Doublets.Cli.Tests/Foundation.Data.Doublets.Cli.Tests.csproj \
  --configuration Release -m:1 -p:UseSharedCompilation=false \
  --filter 'FullyQualifiedName~CausalQueryTests|FullyQualifiedName~GenericLinksOperationTests|FullyQualifiedName~LinoProtocolCodecTests|FullyQualifiedName~LinksServerTests|FullyQualifiedName~RemoteLinksTests'
