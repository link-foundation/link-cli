# Preserve each native C# platform's coverage

R8 requires measured coverage as well as passing tests. Every native C# test
job already collects complete Cobertura, but only Linux currently publishes
its summary and report. This hides Windows-only file-handle branches from
review of the available coverage evidence.

- [x] Inspect successful exact-head run 40491222901 and all its artifacts:
      only one csharp-coverage artifact is retained. The unchanged workflow
      gates Node setup, summary and retention to ubuntu-24.04, even though
      Windows/macOS also collect coverage during their full test step.
- [x] Publish each platform's existing full report and summary, preserving
      the established Linux artifact name and Codecov behavior. Additional
      platform artifacts must have distinct names. Remove no source or
      instrumentation denominator and do not sum percentages across reports.
- [x] Run repository checks, actionlint/ShellCheck and whitespace checks.
- [ ] Inspect fresh exact-head three-platform CI and download actual artifacts;
      retain their complete platform metrics and any failed-test logs.

This is evidence retention, not newly achieved 100% coverage. Native/C#/
complete linked-WASM 100% and every wider unmet criterion remain unfinished.
The PR remains draft/unmerged.

The reusable artifact verifier independently fails against successful dec6cb7
with both additional platform reports missing. It checks exact run/head and
job conclusions, per-platform full-suite totals, summary presence, archive
size/SHA256 and complete XML metrics when those reports become available.
See csharp-platform-coverage-before.txt for actual baseline evidence.

The complete local validation ending 2026-10-27T11:50:17Z includes this workflow
change: all 221 repository checks, dependency freshness, actionlint/ShellCheck
and whitespace checks pass. Fresh CI must still verify the actual additional
platform artifacts; no coverage improvement is inferred from a YAML edit.
