#!/usr/bin/env python3
"""Rewrite every `actions/cache@vN` step into restore + main-only save (issue #172).

Usage: python3 -I experiments/issue-172/split-cache-steps.py .github/workflows/*.yml

The combined action saves in its post step on every run whose primary key
missed, including pull requests. Pull request caches are scoped to
refs/pull/N/merge and cannot be restored by main or by other pull requests,
so multi-GB rust/target caches only evicted each other under the 10 GB
repository quota, and the Windows test job spent 3-5 minutes compressing a
~3 GB archive inside its 45-minute budget (run 42877437350 was cancelled in
that post step after every test passed).
"""
import re
import sys

SAVE_IF = (
    "${{ github.event_name == 'push' && github.ref == 'refs/heads/main' "
    "&& steps.%s.outputs.cache-hit != 'true' }}"
)


def job_end(lines, start):
    """Index of the first line after the job containing `start`."""
    for index in range(start + 1, len(lines)):
        line = lines[index]
        if line.strip() == "":
            continue
        indent = len(line) - len(line.lstrip())
        if indent <= 2:
            # Back up over trailing blank lines and section comments.
            end = index
            while end > start and (
                lines[end - 1].strip() == "" or lines[end - 1].lstrip().startswith("#")
                and len(lines[end - 1]) - len(lines[end - 1].lstrip()) <= 2
            ):
                end -= 1
            return end
    end = len(lines)
    while lines[end - 1].strip() == "":
        end -= 1
    return end


def rewrite(text):
    lines = text.split("\n")
    count = 0
    while True:
        uses = next(
            (i for i, l in enumerate(lines) if re.match(r"\s+uses: actions/cache@v\d+\s*$", l)),
            None,
        )
        if uses is None:
            break
        count += 1
        name_line = uses - 1
        name_match = re.match(r"(\s+)- name: (.*)$", lines[name_line])
        assert name_match, lines[name_line]
        dash_indent = name_match.group(1)
        version = re.search(r"@(v\d+)", lines[uses]).group(1)
        # Collect the `with:` block to reuse its path for the save step.
        index = uses + 1
        block = []
        while index < len(lines) and (
            lines[index].strip() == "" and False
            or len(lines[index]) - len(lines[index].lstrip()) > len(dash_indent)
        ):
            block.append(lines[index])
            index += 1
        text_block = "\n".join(block)
        step_id = "cache-%d" % count
        path_match = re.search(r"\n?(\s+path: [\s\S]*?)(?=\n\s+key:)", "\n" + text_block)
        assert path_match, text_block
        lines[name_line] = f"{dash_indent}- name: Restore {name_match.group(2)[0].lower()}{name_match.group(2)[1:]}"
        lines[uses] = f"{dash_indent}  id: {step_id}\n{dash_indent}  uses: actions/cache/restore@{version}"
        end = job_end(lines, uses)
        save = [
            "",
            f"{dash_indent}# Pull request caches are scoped to their merge ref, so only main",
            f"{dash_indent}# saves; every branch restores main's cache (issue #172).",
            f"{dash_indent}- name: Save {name_match.group(2)[0].lower()}{name_match.group(2)[1:]}",
            f"{dash_indent}  if: " + SAVE_IF % step_id,
            f"{dash_indent}  uses: actions/cache/save@{version}",
            f"{dash_indent}  with:",
            path_match.group(1),
            f"{dash_indent}    key: ${{{{ steps.{step_id}.outputs.cache-primary-key }}}}",
        ]
        lines[end:end] = save
    return "\n".join(lines), count


for path in sys.argv[1:]:
    with open(path) as handle:
        original = handle.read()
    updated, count = rewrite(original)
    if count:
        with open(path, "w") as handle:
            handle.write(updated)
        print(f"{path}: {count} cache step(s)")
