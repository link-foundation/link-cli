#!/usr/bin/env bash
# Checks the Platform.Data.Doublets `MergeUsages` defect of 0.18.1, fixed in
# 0.18.6. Exits 0 while the fix holds, and non-zero if the defect comes back.
set -u
cd "$(dirname "$0")"
dotnet run -v q --nologo
