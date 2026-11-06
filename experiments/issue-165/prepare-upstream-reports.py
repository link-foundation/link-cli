import json
from pathlib import Path

root = Path("dev/log/issues/165/pulls/166")
reports = root / "analysis/upstream-reports"
reports.mkdir(exist_ok=True)
source = "https://github.com/link-foundation/link-cli/pull/166"
repro = "https://github.com/link-foundation/link-cli/blob/issue-165-a5206aa5b4c0/experiments/issue-165/reproduce-template-gaps.mjs"
items = []
for lang in ["rust", "js", "csharp", "python"]:
    repo = f"link-foundation/{lang}-ai-driven-development-pipeline-template"
    meta = json.loads((root / f"templates/{lang}/source.json").read_text())
    print(lang, meta)

for lang in ["rust", "js", "csharp", "python"]:
    meta = json.loads((root / f"templates/{lang}/source.json").read_text())
    repo = meta["repository"]
    sha = meta["sha"]
    sections = []
    if lang != "python":
        sections.append(f"""The lychee report parser in [`scripts/recheck-broken-links.mjs`](https://github.com/{repo}/blob/{sha}/scripts/recheck-broken-links.mjs) consumes an adjacent failure as part of the previous record. With this report:

```text
## Errors per input
* [404] https://example.com/missing
* [ERROR] https://example.com/reset
```

`parseLycheeFailures(report)` returns **one** record, whose trailing description contains `* [ERROR] https://example.com/reset`, instead of two. The record regex uses `\\s*` after the URL; it consumes the newline, and `[^\\n]*` then consumes the next record. That can bypass the bounded retry/archive decision for the second URL.

Reproduce by importing `parseLycheeFailures` from the linked script and passing the literal report above. The downstream offline reproducer below asserts the current one-record result against this exact snapshot and verifies the fixed parser returns both records.

Workaround/code fix: replace the separator's `\\s*` with `[ \\t]*`, keep the description on the same line, and restrict parsing to the Errors section. Add adjacent `[404]` / `[ERROR]` and no-description cases to the existing parser tests.""")
    if lang != "csharp":
        filename = (
            "check_codeql_results.py"
            if lang == "python"
            else "check-codeql-results.mjs"
        )
        command = "python3" if lang == "python" else "node"
        sections.append(f"""The SARIF gate in [`scripts/{filename}`](https://github.com/{repo}/blob/{sha}/scripts/{filename}) accepts an incomplete extraction. Put this into `repro/example.sarif` and run `{command} scripts/{filename} repro`:

```json
{{"version":"2.1.0","runs":[{{"tool":{{"driver":{{"name":"CodeQL"}}}},"results":[],"invocations":[{{"executionSuccessful":true,"toolExecutionNotifications":[{{"level":"error","message":{{"text":"Extraction failed"}}}}]}}]}}]}}
```

Actual: exit 0. Expected: nonzero, because an empty findings list does not prove source extraction completed. Real CodeQL SARIF from downstream CodeQL 2.27.4 exhibits exactly this combination (`executionSuccessful: true` plus extraction error notifications).

Workaround/code fix: validate every invocation's `toolExecutionNotifications` and `toolConfigurationNotifications`; fail on error, warning, missing level (SARIF defaults to warning), malformed records and malformed notification arrays. Keep note/none informational notifications allowed, and print the diagnostic descriptor, location and message. Include regression tests for both notification arrays even when `results` is empty. This is separate from gating ordinary unresolved vulnerability findings; that gate already exists.""")
    title = (
        "CI gates drop adjacent lychee failures and accept CodeQL extraction errors"
        if lang in ["rust", "js"]
        else "Lychee parser consumes adjacent failure records"
        if lang == "csharp"
        else "SARIF gate accepts CodeQL extraction errors with successful invocation"
    )
    body = (
        f"""Found while auditing {source} against the complete template tree at `{sha}`.

"""
        + "\n\n".join(sections)
        + f"""

Reproducible downstream comparison: [offline reproducer]({repro}), run from the link-cli checkout with `node experiments/issue-165/reproduce-template-gaps.mjs`. It uses the committed immutable source snapshots, performs no network requests, demonstrates the template failures, and verifies the downstream fixes. [Collected evidence and analysis](https://github.com/link-foundation/link-cli/tree/issue-165-a5206aa5b4c0/dev/log/issues/165/pulls/166).
"""
    )
    path = reports / f"{lang}-template.md"
    path.write_text(body)
    items.append({"repository": repo, "title": title, "body_file": str(path)})

path = reports / "codeql-await-using.md"
path.write_text("""CodeQL 2.27.4 reports a syntax error for a valid call to a function named `using` when it is awaited. Found by checking extraction diagnostics in https://github.com/link-foundation/link-cli/pull/166.

Minimal ECMAScript module:

```js
async function using(store, action) { return action(store); }
await using({}, async () => {});
```

`node --input-type=module < repro.mjs` succeeds (Node 24 locally; Node 26, Bun and Deno accepted the original downstream conformance suite). CodeQL JavaScript extraction reports `Syntax error: unexpected token '('` at the call, and SARIF includes `js/syntax-error` despite an empty results array and successful invocation. Real failing run: https://github.com/link-foundation/link-cli/actions/runs/42130454613 (CodeQL javascript-typescript).

The apparent parser cause is `javascript/extractor/src/com/semmle/js/parser/Parser.java`: `isAwaitUsing()` checks only the words `await using`, and `parseStatement()` then selects `parseVarStatement(..., "using")` without requiring a following binding identifier. This misclassifies the call as an await-using declaration. `isUsingDecl()` already has binding lookahead for unawaited `using`.

Expected: parse the expression statement as an awaited function call. Suggested fix: require valid declaration binding lookahead after `await using` before choosing the declaration production; retain actual `await using resource = expression` parsing. Add both forms as parser regression fixtures.

Workaround: rename the helper (downstream uses `withClient`) while keeping all conformance scenarios. Reproducer and collected SARIF: https://github.com/link-foundation/link-cli/tree/issue-165-a5206aa5b4c0/dev/log/issues/165/pulls/166 . The standalone reproducer is `experiments/issue-165/codeql-using-repro.mjs.txt` (text suffix prevents the known parser defect from breaking its own audit).
""")
items.append(
    {
        "repository": "github/codeql",
        "title": "JavaScript parser treats awaited using(...) call as await-using declaration",
        "body_file": str(path),
    }
)
path = reports / "codeql-csharp-buildless.md"
path.write_text("""Buildless C# extraction combines independent runnable programs and emits CS8802 when multiple projects use top-level statements. Found with CodeQL 2.27.4 / codeql-action v4 in https://github.com/link-foundation/link-cli/pull/166.

Reproduction shape: a repository with `a/a.csproj` and `b/b.csproj`, both SDK console projects, each with its own `Program.cs` containing `System.Console.WriteLine("hello");`. Each project builds individually. Analyze the repository with CodeQL C#, `build-mode: none`, and inspect SARIF extraction diagnostics (not only results/upload status).

Actual downstream run: https://github.com/link-foundation/link-cli/actions/runs/42130454613 . Re-running the C# job with debug output showed five `CS8802: Only one compilation unit can have top-level statements` messages, one for each independent example/probe outside the primary solution. See `ci-logs/codeql-csharp-debug.log.gz`, original SARIF, and line-numbered diagnostics under https://github.com/link-foundation/link-cli/tree/issue-165-a5206aa5b4c0/dev/log/issues/165/pulls/166 . Extraction otherwise says SUCCEEDED and SARIF says `executionSuccessful: true`.

Expected: independent projects' top-level programs are analyzed in separate compilations, without source-level compiler errors caused by merging unrelated entry points. Suggested code fix: partition buildless compilations by owning project (including separate implicit Program types), or explicitly handle multiple top-level entry points in the buildless compilation model. Add a two-project console fixture.

Workarounds: use explicit namespaced Program.Main entry points (the downstream fix retains every example), or use manual build extraction of the separate projects when feasible. Excluding actual examples would lose security coverage, so we did not use that workaround.

The two-project shape above is inferred from the confirmed five-file repository reproduction; the exact tiny fixture has not been run through a separate CodeQL installation. The downstream CI log and diagnostic locations are the measured reproducer.
""")
items.append(
    {
        "repository": "github/codeql",
        "title": "C# buildless extraction reports CS8802 for independent top-level example programs",
        "body_file": str(path),
    }
)
(root / "analysis/upstream-report-plan.json").write_text(
    json.dumps(items, indent=2) + "\n"
)
