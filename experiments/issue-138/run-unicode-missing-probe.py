"""Verify the published before source and restore edits even on a build failure.

Run sequentially with every compiler. --before uses the three affected files
from cache head 4729b53; no checkout, reset, commit or history rewrite is used.
"""
import os
from pathlib import Path
import resource
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
logs = root / "experiments/issue-138"
before = sys.argv[1:] == ["--before"]
assert not sys.argv[1:] or before
label = "before" if before else "after"
paths = [root / "rust/src" / name for name in [
    "sequences/right_sequence_walker.rs",
    "sequences/unicode_sequence_to_string_converter.rs",
    "unicode_string_storage.rs",
]]
originals = {path: path.read_bytes() for path in paths}
env = {**os.environ, "CARGO_BUILD_JOBS": "1", "CARGO_PROFILE_DEV_DEBUG": "0"}

def limits():
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    resource.setrlimit(resource.RLIMIT_AS, (128 * 1024 * 1024,) * 2)
    resource.setrlimit(resource.RLIMIT_CPU, (2, 2))

try:
    if before:
        for path in paths:
            path.write_bytes(subprocess.check_output([
                "git", "show", "4729b539ddd9db9167895fed654282c95789e026:" + str(path.relative_to(root))
            ], cwd=root))
    with (logs / f"unicode-missing-{label}-build.log").open("w") as log:
        subprocess.run(["cargo", "build", "--manifest-path", "rust/Cargo.toml", "--lib"],
            cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
        deps = root / "rust/target/debug/deps"
        library = max(deps.glob("liblink_cli-*.rlib"), key=lambda path: path.stat().st_mtime)
        binary = logs / ("unicode-missing-" + label)
        subprocess.run(["rustc", "--edition=2021", str(logs / "unicode-missing-probe.rs"),
            "--extern", "link_cli=" + str(library), "-L", "dependency=" + str(deps),
            "-o", str(binary)], stdout=log, stderr=subprocess.STDOUT, check=True)
    with (logs / f"unicode-missing-{label}.log").open("w") as log:
        child = subprocess.Popen([str(binary)], stdout=log, stderr=subprocess.STDOUT, preexec_fn=limits)
        try:
            code = child.wait(timeout=3)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()
            raise AssertionError("finite missing-node probe did not stop")
    output = (logs / f"unicode-missing-{label}.log").read_text()
    print(output)
    if before:
        assert code == 101 and 'string=Ok("Z"); code_units=Ok([90])' in output
        print("Confirmed published source silently discarded the missing child.")
    else:
        assert code == 0, code
        print("Both public decoders reject the missing child.")
finally:
    for path, content in originals.items():
        path.write_bytes(content)
    assert all(path.read_bytes() == content for path, content in originals.items())
