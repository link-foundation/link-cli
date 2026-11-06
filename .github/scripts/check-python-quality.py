"""Run cheap lint, format and CI-helper type checks over Git's maintained files."""

import subprocess
import sys
from pathlib import Path

root = Path(__file__).resolve().parents[2]
tracked = subprocess.check_output(
    ["git", "ls-files", "-z", "--", "*.py"], cwd=root, text=True
).split("\0")
files = [
    file
    for file in tracked
    if file and not file.startswith(("dev/log/", "docs/case-studies/"))
]
if not files:
    raise RuntimeError("No tracked Python files; no checks performed")
for file in files:
    if len((root / file).read_text().splitlines()) > 1000:
        raise RuntimeError(f"{file} exceeds the 1000-line source limit")
config = ".github/scripts/python-quality.toml"
for args in [
    ["ruff", "check", "--config", config, *files],
    ["ruff", "format", "--check", "--config", config, *files],
    [
        "mypy",
        "--strict",
        ".github/scripts/unacquired-jobs.py",
        ".github/scripts/check-python-quality.py",
    ],
]:
    subprocess.run([sys.executable, "-m", *args], cwd=root, check=True)
print(
    f"Python quality: {len(files)} files checked; runtime CI helpers checked with mypy --strict"
)
