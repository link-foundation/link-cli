"""Compress immutable evidence for review without dropping original bytes."""

import gzip
import shutil
import tarfile
from pathlib import Path

root = Path("dev/log/issues/165/pulls/166")
for directory in (root / "templates").iterdir():
    sources = [
        path
        for path in directory.rglob("*")
        if path.is_file()
        and path
        not in [
            directory / name
            for name in (
                "tree.json",
                "source.json",
                "selected-files.txt",
                "snapshot.tar.gz",
            )
        ]
    ]
    if sources:
        with tarfile.open(
            directory / "snapshot.tar.gz", "w:gz", compresslevel=6
        ) as archive:
            for path in sorted(sources):
                archive.add(
                    path,
                    arcname=f"{directory.name}/{path.relative_to(directory)}",
                    recursive=False,
                )
        for path in sources:
            path.unlink()
for path in (root / "ci-logs").rglob("*.log"):
    with (
        path.open("rb") as source,
        gzip.open(f"{path}.gz", "wb", compresslevel=6) as destination,
    ):
        shutil.copyfileobj(source, destination)
    path.unlink()
print("Immutable template source and raw CI logs compressed losslessly")
