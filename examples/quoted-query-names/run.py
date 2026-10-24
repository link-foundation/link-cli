#!/usr/bin/env python3
"""Check numeric query names and export replay against either clink executable."""

import argparse
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("clink", nargs="+", help="Executable and optional arguments")
    command = parser.parse_args().clink
    with tempfile.TemporaryDirectory(prefix="clink-quoted-names-") as directory:
        root = Path(directory)
        source = root / "source.links"
        exported = root / "export.lino"
        replayed = root / "replayed.lino"

        def run(database, *arguments):
            result = subprocess.run(
                [*command, "--db", str(database), *arguments],
                capture_output=True, text=True, timeout=60, check=False,
            )
            if result.returncode:
                raise RuntimeError(f"clink failed: {result.stdout}\n{result.stderr}")

        run(source, "() ((a: a a) (b: b b))")
        run(
            source, "--auto-create-missing-references",
            "() ((p: x \"1\") (p2: x '2') (p3: x \"99\") "
            "(exitCode: x \"0\") (pid: x \"12345\") (address: x 1))",
            "--out", str(exported),
        )
        text = exported.read_text(encoding="utf-8")
        for expected in (
            "(p: x '1')", "(p2: x '2')", "(p3: x '99')",
            "(exitCode: x '0')", "(pid: x '12345')", "(address: x a)",
        ):
            assert expected in text, f"Missing {expected}:\n{text}"
        run(root / "destination.links", f"() ({text})", "--out", str(replayed))
        assert text == replayed.read_text(encoding="utf-8")
        print(text, end="")
        print("Quoted names, bare addresses, and strict export replay verified.")


if __name__ == "__main__":
    main()
