"""Compile only after other compilers finish; bound a suspected cyclic walk."""
from pathlib import Path
import resource
import subprocess

root = Path(__file__).resolve().parent
deps = root.parent.parent / "rust/target/debug/deps"
library = max(deps.glob("liblink_cli-*.rlib"), key=lambda entry: entry.stat().st_mtime)
binary = root / "unicode-cycle-probe"
with (root / "unicode-cycle-probe-build.log").open("w") as log:
    subprocess.run(["rustc", "--edition=2021", str(root / "unicode-cycle-probe.rs"),
        "--extern", "link_cli=" + str(library), "-L", "dependency=" + str(deps),
        "-o", str(binary)], stdout=log, stderr=subprocess.STDOUT, check=True)

def limits():
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    resource.setrlimit(resource.RLIMIT_AS, (256 * 1024 * 1024,) * 2)
    resource.setrlimit(resource.RLIMIT_CPU, (2, 2))

with (root / "unicode-cycle-probe.log").open("w") as log:
    child = subprocess.Popen([str(binary)], stdout=log, stderr=subprocess.STDOUT,
        preexec_fn=limits)
    try:
        code = child.wait(timeout=3)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait()
        log.write("finite parent deadline exceeded; child killed and reaped\n")
        code = 124
print("bounded Unicode cycle exit:", code)
print((root / "unicode-cycle-probe.log").read_text())
raise SystemExit(code != 0)
