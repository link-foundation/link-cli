#!/usr/bin/env bash
# Reproduces the Platform.Data.Doublets 0.18.1 `MergeUsages` defect.
# Exits 0 while the defect reproduces, and non-zero once upstream fixes it --
# which is the signal to drop the parity exemption in ../cli-parity/run.sh.
set -u
cd "$(dirname "$0")"
dotnet run -v q --nologo
