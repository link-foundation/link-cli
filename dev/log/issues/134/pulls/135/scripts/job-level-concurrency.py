"""Replace workflow-level concurrency with job-level groups (issue #134).

Read-only jobs get `check-<workflow>-<ref>-<job>[-<matrix>]` groups that are
cancelled when superseded on pull requests; jobs that already declare a
(writer) group keep it. Usage: job-level-concurrency.py <repo-root>
"""
import re, sys

root = sys.argv[1]
WORKFLOW_COMMENT = {
    'csharp': 3, 'rust': 3, 'js-client': 1, 'docs': 0, 'wasm': 0,
}
NOTE = (
    "# Concurrency is set per job (issue #134): superseded read-only checks are\n"
    "# cancelled on pull requests, while writer jobs share a never-cancelled\n"
    "# group. A workflow-level group held the whole run, including the release\n"
    "# and the registry wait, so a third push to main cancelled the queued run.\n"
)
WRITERS = {'js-client': {'release': 'npm-writer'}}

for wf, comment_lines in WORKFLOW_COMMENT.items():
    path = f'{root}/.github/workflows/{wf}.yml'
    lines = open(path).read().split('\n')
    # Drop the workflow-level block and the comment directly above it.
    idx = lines.index('concurrency:')
    end = idx + 1
    while end < len(lines) and lines[end].startswith('  '):
        end += 1
    if end < len(lines) and lines[end] == '':
        end += 1
    start = idx - comment_lines
    assert all(l.startswith('#') for l in lines[start:idx]), (wf, lines[start:idx])
    lines[start:end] = NOTE.rstrip('\n').split('\n') + ['']
    text = '\n'.join(lines)

    head, jobs = text.split('\njobs:\n', 1)
    blocks = re.split(r'(?m)^(?=  [A-Za-z0-9_-]+:\s*$)', jobs)
    out = []
    for block in blocks:
        m = re.match(r'  ([A-Za-z0-9_-]+):\s*\n', block)
        if not m or re.search(r'(?m)^    concurrency:', block):
            out.append(block)
            continue
        job = m.group(1)
        writer = WRITERS.get(wf, {}).get(job)
        if writer:
            conc = ("    # Writer group: never cancel a publish that may already be under way.\n"
                    "    concurrency:\n"
                    f"      group: {writer}-${{{{ github.repository }}}}\n"
                    "      cancel-in-progress: false\n")
        else:
            matrix = re.search(r'(?m)^      matrix:\n((?:        .*\n)+)', block)
            suffix = ''
            if matrix:
                keys = re.findall(r'(?m)^        ([A-Za-z0-9_-]+):', matrix.group(1))
                keys = [k for k in keys if k not in ('include', 'exclude')]
                suffix = ''.join(f'-${{{{ matrix.{k} }}}}' for k in keys)
            conc = ("    concurrency:\n"
                    f"      group: check-${{{{ github.workflow }}}}-${{{{ github.ref }}}}-{job}{suffix}\n"
                    "      cancel-in-progress: ${{ github.ref != 'refs/heads/main' }}\n")
        anchor = re.search(r'(?m)^    timeout-minutes:.*\n', block) or re.search(r'(?m)^    runs-on:.*\n', block)
        assert anchor, (wf, job)
        block = block[:anchor.end()] + conc + block[anchor.end():]
        out.append(block)
    open(path, 'w').write(head + '\njobs:\n' + ''.join(out))
    print(wf, 'updated')
