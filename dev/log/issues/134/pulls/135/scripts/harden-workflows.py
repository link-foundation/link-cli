"""One-off transformer used for issue #134 (kept for reproducibility).

Applies the mechanical part of the workflow hardening to .github/workflows:

* `actions/checkout` steps get `persist-credentials: false`, except in the
  jobs that push commits or tags (they keep the token explicitly, with
  `persist-credentials: true`, so the intent is visible to readers and zizmor);
* `dtolnay/rust-toolchain@stable` (an unpinned third-party action) becomes a
  plain `rustup` step, which every GitHub-hosted runner image already ships;
* the remaining third-party actions are pinned to the commit of their
  latest release, with the release tag as a comment.

Usage: python3 -I dev/log/issues/134/pulls/135/scripts/harden-workflows.py <repo-root>
"""

import re
import sys
from pathlib import Path

PUSHING_JOBS = {
    "csharp.yml": {"release", "instant-release", "changeset-pr"},
    "rust.yml": {"auto-release", "manual-release", "changelog-pr"},
}

PINS = {
    "taiki-e/install-action@v2": "taiki-e/install-action@1e29709c0ec63babbe8f266397a465f29a825f91 # v2.87.31",
    "denoland/setup-deno@v2": "denoland/setup-deno@22d081ff2d3a40755e97629de92e3bcbfa7cf2ed # v2.0.5",
}


def current_job(lines, index):
    for line in reversed(lines[:index]):
        match = re.match(r"^  ([A-Za-z0-9_-]+):\s*$", line)
        if match:
            return match.group(1)
    return None


def harden(name, text):
    lines = text.split("\n")
    out = []
    i = 0
    while i < len(lines):
        line = lines[i]
        checkout = re.match(r"^(\s*)- uses: actions/checkout@v\d+\s*$", line)
        if checkout:
            indent = checkout.group(1) + "  "
            persist = "true" if current_job(lines, i) in PUSHING_JOBS.get(name, set()) else "false"
            out.append(line)
            i += 1
            if i < len(lines) and lines[i] == f"{indent}with:":
                out.append(lines[i])
                out.append(f"{indent}  persist-credentials: {persist}")
                i += 1
            else:
                out.append(f"{indent}with:")
                out.append(f"{indent}  persist-credentials: {persist}")
            continue
        toolchain = re.match(r"^(\s*)uses: dtolnay/rust-toolchain@stable\s*$", line)
        if toolchain:
            indent = toolchain.group(1)
            components, targets = [], []
            i += 1
            if i < len(lines) and lines[i] == f"{indent}with:":
                i += 1
                while i < len(lines) and lines[i].startswith(f"{indent}  "):
                    key, _, value = lines[i].strip().partition(":")
                    values = [v.strip() for v in value.split(",") if v.strip()]
                    (components if key == "components" else targets).extend(values)
                    i += 1
            command = "rustup toolchain install stable --profile minimal --no-self-update"
            if components:
                command += " --component " + ",".join(components)
            if targets:
                command += " --target " + ",".join(targets)
            out.append(f"{indent}shell: bash")
            out.append(f"{indent}run: |")
            out.append(f"{indent}  {command}")
            out.append(f"{indent}  rustup default stable")
            continue
        for unpinned, pinned in PINS.items():
            line = re.sub(re.escape(unpinned) + r"\s*$", pinned, line)
        out.append(line)
        i += 1
    return "\n".join(out)


def main():
    root = Path(sys.argv[1])
    for path in sorted((root / ".github" / "workflows").glob("*.yml")):
        before = path.read_text()
        after = harden(path.name, before)
        if after != before:
            path.write_text(after)
            print(f"updated {path.relative_to(root)}")


if __name__ == "__main__":
    main()
