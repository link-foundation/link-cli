"""Exercise the original and fixed Rust detectors on lock/helper-only commits."""

import json
import os
import resource
import subprocess
from pathlib import Path
from tempfile import TemporaryDirectory

root = Path(__file__).resolve().parents[2]
original = subprocess.check_output(
    ["git", "show", "8f24ffd:rust/scripts/detect-code-changes.rs"], cwd=root
)
fixed = (root / "rust/scripts/detect-code-changes.rs").read_bytes()
with TemporaryDirectory(prefix="issue-165-change-detection-") as temporary:
    work = Path(temporary)
    (work / "before.rs").write_bytes(original)
    (work / "after.rs").write_bytes(fixed)
    (work / "Cargo.toml").write_text(
        '[package]\nname="issue-165-detector-repro"\nversion="0.0.0"\nedition="2021"\n'
        '[dependencies]\nregex="1"\n'
        '[[bin]]\nname="before"\npath="before.rs"\n'
        '[[bin]]\nname="after"\npath="after.rs"\n'
    )
    env = {
        **os.environ,
        "CARGO_BUILD_JOBS": "1",
        "CARGO_PROFILE_DEV_DEBUG": "0",
        "CARGO_PROFILE_TEST_DEBUG": "0",
        "CARGO_INCREMENTAL": "0",
        "GITHUB_EVENT_NAME": "push",
        "GITHUB_OUTPUT": "",
    }
    # The detector itself appends outputs when this variable exists.
    env.pop("GITHUB_OUTPUT")
    subprocess.run(
        [
            "cargo",
            "build",
            "--manifest-path",
            str(work / "Cargo.toml"),
            "--bins",
            "-j",
            "1",
        ],
        env=env,
        check=True,
    )
    subprocess.run(
        [
            "cargo",
            "test",
            "--manifest-path",
            str(work / "Cargo.toml"),
            "--bin",
            "after",
            "-j",
            "1",
        ],
        env=env,
        check=True,
    )
    fixture = work / "fixture"
    fixture.mkdir()

    def git(*args):
        subprocess.run(["git", *args], cwd=fixture, check=True, capture_output=True)

    git("init", "-b", "main")
    git("config", "user.name", "Regression test")
    git("config", "user.email", "test@example.invalid")
    (fixture / "README.md").write_text("Fixture\n")
    git("add", ".")
    git("commit", "-m", "Initial fixture")
    for filename, output in [
        ("rust/Cargo.lock", "toml-changed"),
        ("rust/scripts/check-scripts.sh", "workflow-changed"),
    ]:
        path = fixture / filename
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("# Changed input\n")
        git("add", filename)
        git("commit", "-m", f"Change {filename} only")
        results = {}
        for name in ["before", "after"]:
            stdout = subprocess.check_output(
                [str(work / "target/debug" / name)], cwd=fixture, env=env, text=True
            )
            results[name] = f"{output}=true" in stdout.splitlines()
        assert results == {"before": False, "after": True}, results
        print(filename, json.dumps(results), flush=True)
    for label, cwd, extra_env in [
        ("missing Git evidence", work, {}),
        ("unwritable Actions output", fixture, {"GITHUB_OUTPUT": str(fixture)}),
    ]:
        results = {}
        for name in ["before", "after"]:
            result = subprocess.run(
                [str(work / "target/debug" / name)],
                cwd=cwd,
                env={**env, **extra_env},
                capture_output=True,
                text=True,
            )
            results[name] = result.returncode
        assert results == {"before": 0, "after": 1}, results
        print(label, json.dumps(results), flush=True)
print("Peak child memory KiB:", resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss)
