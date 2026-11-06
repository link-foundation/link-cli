import concurrent.futures
import json
import re
import subprocess
from pathlib import Path

root = Path("dev/log/issues/165/pulls/166/templates")
tasks = []
for lang in ["rust", "csharp", "js", "python"]:
    p = root / lang
    meta = json.loads((p / "source.json").read_text())
    tree = json.loads((p / "tree.json").read_text())["tree"]
    for entry in tree:
        file = entry["path"]
        if entry["type"] != "blob" or (p / file).exists():
            continue
        if file.startswith(("tests/", ".husky/", "eslint-rules/")) or re.search(
            r"(^|/)(?:\.?prettier[^/]*|eslint[^/]*|\.editorconfig|\.ruff.toml|global.json|jsconfig.json|tsconfig[^/]*|Directory.Build.props|\.secretlintrc.json|CONTRIBUTING.md|deno.json|bunfig.toml)$",
            file,
        ):
            tasks.append((meta, p, file))


def collect(task):
    meta, p, file = task
    result = subprocess.run(
        [
            "gh",
            "api",
            f"repos/{meta['repository']}/contents/{file}?ref={meta['sha']}",
            "-H",
            "Accept: application/vnd.github.raw+json",
        ],
        capture_output=True,
    )
    if result.returncode:
        raise RuntimeError(result.stderr.decode())
    dest = p / file
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(result.stdout)
    return f"{meta['repository']} {file}"


with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
    for result in pool.map(collect, tasks):
        print(result, flush=True)
