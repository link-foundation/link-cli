Buildless C# extraction combines independent runnable programs and emits CS8802 when multiple projects use top-level statements. Found with CodeQL 2.27.4 / codeql-action v4 in https://github.com/link-foundation/link-cli/pull/166.

Reproduction shape: a repository with `a/a.csproj` and `b/b.csproj`, both SDK console projects, each with its own `Program.cs` containing `System.Console.WriteLine("hello");`. Each project builds individually. Analyze the repository with CodeQL C#, `build-mode: none`, and inspect SARIF extraction diagnostics (not only results/upload status).

Actual downstream run: https://github.com/link-foundation/link-cli/actions/runs/42130454613 . Re-running the C# job with debug output showed five `CS8802: Only one compilation unit can have top-level statements` messages, one for each independent example/probe outside the primary solution. See `ci-logs/codeql-csharp-debug.log.gz`, original SARIF, and line-numbered diagnostics under https://github.com/link-foundation/link-cli/tree/issue-165-a5206aa5b4c0/dev/log/issues/165/pulls/166 . Extraction otherwise says SUCCEEDED and SARIF says `executionSuccessful: true`.

Expected: independent projects' top-level programs are analyzed in separate compilations, without source-level compiler errors caused by merging unrelated entry points. Suggested code fix: partition buildless compilations by owning project (including separate implicit Program types), or explicitly handle multiple top-level entry points in the buildless compilation model. Add a two-project console fixture.

Workarounds: use explicit namespaced Program.Main entry points (the downstream fix retains every example), or use manual build extraction of the separate projects when feasible. Excluding actual examples would lose security coverage, so we did not use that workaround.

The two-project shape above is inferred from the confirmed five-file repository reproduction; the exact tiny fixture has not been run through a separate CodeQL installation. The downstream CI log and diagnostic locations are the measured reproducer.
