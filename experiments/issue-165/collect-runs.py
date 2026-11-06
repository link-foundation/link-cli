"""Collect issue-referenced workflow evidence without building the project."""

import json
import re
import subprocess
import sys
from pathlib import Path

root = Path("dev/log/issues/165/pulls/166")
issue = json.loads((root / "github/issue-165.json").read_text())
ids = list(dict.fromkeys(re.findall(r"actions/runs/(\d+)", issue["body"])))
ids = sys.argv[1:] or ids + ["42128585418", "42128585421"]
for run_id in ids:
    metadata = subprocess.check_output(
        [
            "gh",
            "run",
            "view",
            run_id,
            "--repo",
            "link-foundation/link-cli",
            "--json",
            "databaseId,name,headSha,createdAt,updatedAt,conclusion,status,jobs",
        ]
    )
    run = json.loads(metadata)
    (root / f"github/run-{run_id}.json").write_bytes(metadata)
    if run["status"] != "completed":
        print(run_id, run["status"], flush=True)
        continue
    with (root / f"ci-logs/run-{run_id}.log").open("w") as output:
        result = subprocess.run(
            [
                "gh",
                "run",
                "view",
                run_id,
                "--repo",
                "link-foundation/link-cli",
                "--log",
            ],
            stdout=output,
            stderr=subprocess.PIPE,
            text=True,
        )
        if result.returncode:
            (root / f"ci-logs/run-{run_id}-missing.txt").write_text(result.stderr)
            print(run_id, result.stderr.strip(), flush=True)
    for job in run["jobs"]:
        annotations = subprocess.check_output(
            [
                "gh",
                "api",
                f"repos/link-foundation/link-cli/check-runs/{job['databaseId']}/annotations",
                "--paginate",
                "--slurp",
            ]
        )
        (root / f"github/annotations-{job['databaseId']}.json").write_bytes(annotations)
    print(run_id, run["name"], run["conclusion"], flush=True)
