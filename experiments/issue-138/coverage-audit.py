#!/usr/bin/env python3
"""Rank measured gaps without changing the report's denominator.

LLVM locations are zero-hit region starts, including regions on partly covered
lines. Cobertura locations are lines with no hit in any class for that file.
"""
import argparse
import json
from pathlib import Path
import xml.etree.ElementTree as ET

parser = argparse.ArgumentParser()
parser.add_argument("report", type=Path)
parser.add_argument("--limit", type=int, default=25)
args = parser.parse_args()
if not 1 <= args.limit <= 1000:
    parser.error("limit must be in 1..1000")
rows = []
if args.report.suffix == ".json":
    print("Locations: LLVM zero-hit region starts; some lines are partly covered.")
    report = json.loads(args.report.read_text())
    for group in report["data"]:
        for file in group["files"]:
            summary = file["summary"]
            missing = summary["lines"]["count"] - summary["lines"]["covered"]
            if missing:
                locations = sorted({
                    s[0] for s in file["segments"]
                    if s[3] and s[4] and not s[2] and (len(s) < 6 or not s[5])
                })
                rows.append((missing, file["filename"], locations))
else:
    print("Locations: Cobertura lines with no hit in any class for that file.")
    report = ET.parse(args.report).getroot()
    files = {}
    for cls in report.findall(".//class"):
        locations = files.setdefault(cls.attrib["filename"], {})
        for line in cls.findall("./lines/line"):
            number = int(line.attrib["number"])
            locations[number] = locations.get(number, False) or int(line.attrib["hits"]) > 0
    for file, locations in files.items():
        missing = sorted(line for line, covered in locations.items() if not covered)
        if missing:
            rows.append((len(missing), file, missing))
for missing, file, locations in sorted(rows, reverse=True)[:args.limit]:
    print(f"{missing:4} uncovered lines: {file}")
    print("     " + ", ".join(map(str, locations)))
