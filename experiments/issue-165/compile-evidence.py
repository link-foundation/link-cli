"""Compile immutable template inventories and run chronology without a build."""

import csv
import hashlib
import json
import re
import subprocess
import tarfile
from pathlib import Path

root = Path("dev/log/issues/165/pulls/166")
tracked = subprocess.check_output(["git", "ls-files", "-z"], text=True).split("\0")
local = [p for p in tracked if p and not p.startswith("dev/log/")]
counterparts = {Path(p).name.replace("_", "-"): p for p in reversed(local)}
summary = [
    "# Full template tree comparison",
    "",
    "Every blob in all four recursive trees is inventoried in `template-file-matrix.csv`. CI/workflow/helper/config/test snapshots are stored beside each tree at the immutable SHA below. Product source, generated dependencies and template demonstration applications are classified separately; they are not runnable CI in this repository.",
    "",
    "| Template | Commit | Tree blobs | Content snapshots |",
    "|---|---|---:|---:|",
]


def comparison(lang, path):
    lowered = path.lower().replace("_", "-")
    if any(
        word in lowered for word in ("desktop", "docker", "buildx", "javascript-first")
    ):
        return (
            "product-specific",
            "",
            "No desktop/container product or translated JavaScript-first source graph is shipped here; preserve independent Rust/C#/JS products.",
        )
    if (
        "/artifact" in lowered
        or "extract-artifact" in lowered
        or "zip-archive" in lowered
    ):
        return (
            "transport",
            ".github/workflows/",
            "Official upload-artifact@v7/download-artifact@v8 retain native retry/integrity behavior; required outputs fail when absent; do not vendor template SDK bundles.",
        )
    if path.startswith(".github/workflows/"):
        name = Path(path).name
        target = {
            "release.yml": {
                "rust": "rust.yml",
                "csharp": "csharp.yml",
                "js": "js-client.yml",
                "python": "workflows.yml",
            }[lang],
            "rerun-unacquired.yml": "rerun-infrastructure-failures.yml",
        }.get(name, name)
        return (
            "workflow",
            f".github/workflows/{target}",
            "Compare triggers, explicit runtimes, permissions, dependencies, timeout, concurrency and terminal gate; language product differences are intentional.",
        )
    source = counterparts.get(Path(path).name.replace("_", "-"))
    if source:
        return (
            "counterpart",
            source,
            "Compare implementation and regression cases; adapt monorepo roots rather than copying template package identity.",
        )
    families = [
        (
            r"codeql|security-gates",
            ".github/scripts/check-codeql-results.mjs",
            "Strict findings plus extraction/invocation completeness; diagnostic gap reported upstream.",
        ),
        (
            r"pipeline|status-gate|job-cancel",
            ".github/scripts/pipeline-status.mjs; .github/scripts/workflow-policy.test.mjs",
            "All jobs covered; failed/cancelled/empty prerequisites fail and intentional skips are explained.",
        ),
        (
            r"runner|unacquired|rerun",
            ".github/scripts/unacquired-jobs.py",
            "Only jobs with confirmed runner-acquisition annotation and no executed work qualify; bounded attempts and latest-run check.",
        ),
        (
            r"preflight|nuget-auth|anonymous-package|publishing-config",
            ".github/scripts/release-preflight.mjs",
            "PR advisory and strict main authentication; registry-specific proof, unknown is not verified.",
        ),
        (
            r"audit|dependenc",
            ".github/workflows/security.yml; .github/scripts/check-latest-dependencies.mjs",
            "All maintained lock/manifests audited and freshness checked; resolved Python CI tools included.",
        ),
        (
            r"web-archive|broken-link|lychee",
            ".github/scripts/recheck-broken-links.mjs; .github/scripts/check-web-archive.mjs",
            "Port bounded transient retry; parse all adjacent failures and fail on malformed/missing evidence.",
        ),
        (
            r"file.*(size|line)|line.*limit",
            "rust/scripts/check-file-size.rs; csharp/scripts/check-file-size.mjs; js/eslint.config.mjs; .github/scripts/check-python-quality.py",
            "1000-line maintained source budgets; immutable history/generated bundles excluded.",
        ),
        (
            r"fresh-merge",
            ".github/scripts/simulate-fresh-merge.sh",
            "Compile/test the actual merge with current base for Rust and C#.",
        ),
        (
            r"lint|format|syntax|eslint|prettier|mypy|ruff|tsconfig|jsconfig|editorconfig|husky|pre-commit|git-hook",
            ".github/workflows/workflows.yml; .pre-commit-config.yaml; js/eslint.config.mjs",
            "ESLint/Prettier, Ruff/mypy, rustfmt/Clippy and dotnet-format mirror CI in local hooks; use Git inventories.",
        ),
        (
            r"push|git-input|release-write",
            "rust/scripts/release-write.rs; csharp/scripts/version-and-commit.mjs",
            "Bound genuine non-fast-forward retries, reject rules/auth failures and report committed only after successful push.",
        ),
        (
            r"changelog|changeset|version|release|publish|wait-for|nuget|crate|package-info|manifest",
            ".github/workflows/rust.yml; .github/workflows/csharp.yml; .github/workflows/js-client.yml",
            "Existing language-scoped release/version/registry/smoke helpers retained; Python has CI helpers but no distributable Python package/PyPI release.",
        ),
        (
            r"docs|pages",
            ".github/workflows/docs.yml",
            "One publisher combines Rust API, C# DocFX and JS workbench; PRs build without publication.",
        ),
        (
            r"secret",
            ".github/scripts/check-secrets.mjs; .secretlintrc.json",
            "Scan every Git-tracked file, including ignored evidence and decompressed log archives.",
        ),
        (
            r"workflow|actionlint|zizmor|shellcheck|tool",
            ".github/workflows/workflows.yml; .github/scripts/workflow-policy.test.mjs",
            "Docker actionlint bundles ShellCheck/Pyflakes; regular and high-confidence pedantic zizmor plus policy tests.",
        ),
    ]
    for pattern, target, reason in families:
        if re.search(pattern, lowered):
            return "adapted-practice", target, reason
    if path.startswith(("tests/", "scripts/", ".github/", "eslint-rules/")):
        return (
            "supporting-helper-or-test",
            ".github/scripts/*.test.mjs; csharp/scripts/*.test.mjs; rust/scripts/test-scripts.sh",
            "Review supporting cases against monorepo equivalent; template-only release layouts and fixtures do not define additional link-cli products.",
        )
    return (
        "product-or-documentation",
        "",
        "Full-tree path reviewed for CI relevance; template application/package identity and documentation are not copied over the existing product.",
    )


with (root / "analysis/template-file-matrix.csv").open("w", newline="") as output:
    writer = csv.writer(output)
    writer.writerow(
        [
            "template",
            "sha",
            "path",
            "bytes",
            "classification",
            "local_counterpart",
            "disposition",
            "snapshot_sha256",
        ]
    )
    for lang in ("rust", "csharp", "js", "python"):
        directory = root / "templates" / lang
        meta = json.loads((directory / "source.json").read_text())
        entries = [
            e
            for e in json.loads((directory / "tree.json").read_text())["tree"]
            if e["type"] == "blob"
        ]
        archived = {}
        archive_path = directory / "snapshot.tar.gz"
        if archive_path.is_file():
            with tarfile.open(archive_path, "r:gz") as archive:
                for member in archive.getmembers():
                    if member.isfile():
                        stream = archive.extractfile(member)
                        if stream is not None:
                            archived[member.name.removeprefix(f"{lang}/")] = (
                                stream.read()
                            )
        snapshots = []
        for entry in entries:
            path = entry["path"]
            source = directory / path
            content = source.read_bytes() if source.is_file() else archived.get(path)
            digest = hashlib.sha256(content).hexdigest() if content is not None else ""
            if digest:
                snapshots.append(path)
            writer.writerow(
                [
                    lang,
                    meta["sha"],
                    path,
                    entry.get("size", ""),
                    *comparison(lang, path),
                    digest,
                ]
            )
        (directory / "selected-files.txt").write_text(
            "\n".join(sorted(snapshots)) + "\n"
        )
        summary.append(
            f"| {lang} | `{meta['sha']}` | {len(entries)} | {len(snapshots)} |"
        )
summary.extend(
    [
        "",
        "The matrix records per-file dispositions, including supporting tests and tool configs. `template-content-inventory.md` records workflow jobs, helper functions and commands inspected in the snapshots. The audit report maps the shared behaviors to fixes and verification. No template-specific desktop, Docker or Python distribution is introduced into link-cli.",
        "",
    ]
)
(root / "analysis/template-comparison.md").write_text("\n".join(summary))

runs = []
for path in (root / "github").glob("run-*.json"):
    run = json.loads(path.read_text())
    runs.append(run)
lines = [
    "# Workflow chronology",
    "",
    "Run creation timestamps are UTC. The SHA is essential: the issue's baseline runs predate its latest main commit. A cancelled job with no runner is not evidence of a failing test. Full logs and check annotations are retained separately.",
    "",
    "| Created UTC | Workflow | Run | SHA | Outcome |",
    "|---|---|---|---|---|",
]
for run in sorted(runs, key=lambda r: (r["createdAt"], r["databaseId"])):
    rid = run["databaseId"]
    lines.append(
        f"| {run['createdAt']} | {run['name']} | [{rid}](https://github.com/link-foundation/link-cli/actions/runs/{rid}) | `{run['headSha'][:12]}` | {run.get('conclusion') or run['status']} |"
    )
lines.append("")
(root / "analysis/timeline.md").write_text("\n".join(lines))
print(f"Compiled four complete template trees and {len(runs)} collected runs")
