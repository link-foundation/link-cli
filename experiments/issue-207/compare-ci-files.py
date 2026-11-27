"""Inventory CI inputs from link-cli and downloaded recursive template trees."""

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TEMPLATES = ("rust", "js", "csharp", "python", "java", "cpp")


def ci_input(path: str) -> bool:
    """Include workflow-owned files, CI helpers, hooks, and root CI policies."""
    return (
        path.startswith((".github/", "scripts/", "parity/"))
        or re.match(r"^(csharp|js|rust)/scripts/", path) is not None
        or re.search(
            r"^\.(pre-commit|lychee|secretlint|actionlint|zizmor|codecov|package-registry-manager)",
            path,
        )
        is not None
    )


def stem(path: str) -> str:
    return Path(path).stem.replace("_", "-")


def purpose(path: str) -> str:
    matched = (
        (
            "change detection and stage gate",
            r"detect|fresh-merge|javascript-first|javascript-stage|parity",
        ),
        (
            "registry and release",
            r"version|changelog|changeset|release|publish|registry|npm|pypi|nuget|crates|vcpkg|maven|package",
        ),
        (
            "workflow validation and status",
            r"workflow|status|gate|policy|concurrency|zizmor|actionlint",
        ),
        (
            "security and dependencies",
            r"security|secrets|codeql|audit|depend|sarif|credential|preflight",
        ),
        ("documentation and links", r"docs|lychee|links|web-archive|pages"),
        (
            "runner and artifact handling",
            r"runner|unacquired|rerun|artifact|archive|budget|buildx",
        ),
        (
            "source and test quality",
            r"lint|format|file-size|line-limit|coverage|warnings|test",
        ),
    )
    for description, pattern in matched:
        if re.search(pattern, path):
            return description
    return "repository or language-specific infrastructure"


def main() -> None:
    directory = Path(sys.argv[1])
    base = sys.argv[2]
    local_tree = subprocess.check_output(
        ["git", "ls-tree", "-rz", base], cwd=ROOT, text=True
    )
    local = {}
    for entry in local_tree.rstrip("\0").split("\0"):
        metadata, path = entry.split("\t", 1)
        if ci_input(path):
            local[path] = metadata.split()[2]
    snapshots = {"link-cli": local}
    template_paths = {}
    for name in TEMPLATES:
        tree = json.loads((directory / f"{name}-tree.json").read_text())["tree"]
        snapshot = {
            file["path"]: file["sha"]
            for file in tree
            if file["type"] == "blob" and ci_input(file["path"])
        }
        template_paths.update(
            {f"{name}/{path}": digest for path, digest in snapshot.items()}
        )
        snapshots[name] = snapshot

    print("repository\tpath\tblob\tpurpose\tother repository matches")
    for name, snapshot in snapshots.items():
        other = template_paths if name == "link-cli" else local
        for path, blob in sorted(snapshot.items()):
            peers = [
                f"{item} ({'identical' if digest == blob else 'same stem'})"
                for item, digest in other.items()
                if digest == blob or stem(item) == stem(path)
            ]
            print(name, path, blob, purpose(path), ", ".join(peers) or "-", sep="\t")


if __name__ == "__main__":
    main()
