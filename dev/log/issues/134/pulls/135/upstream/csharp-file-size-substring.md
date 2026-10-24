## Summary

`scripts/check-file-size.mjs` excludes a file when `bin`, `obj`, `.git`, `node_modules` or `artifacts` occurs **anywhere in its path as a substring**. `src/Robin.cs`, `src/Combiner.cs` and every C# file under `.github/` (`.git` is a substring of `.github`) are silently skipped, and in a checkout whose directory name contains one of the words (a repository called `robin-cli`, checked out as `/home/runner/work/robin-cli/robin-cli`) **every** file is skipped: the gate prints `Checked 0 file(s) - all within the line limit` and exits 0.

Code (template `main` at 4cc372e):

https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/4cc372ebf2075a0c948b46881143f7d66f0f0e7e/scripts/check-file-size.mjs#L15-L24

```js
const EXCLUDE_PATTERNS = ['bin', 'obj', '.git', 'node_modules', 'artifacts'];
export function shouldExclude(path) {
  return EXCLUDE_PATTERNS.some((pattern) => path.includes(pattern));
}
```

## Reproduction

```bash
work=$(mktemp -d)
long() { mkdir -p "$(dirname "$1")"; yes x | head -n 1001 > "$1"; }
long "$work/csharp-app/src/Robin.cs"
long "$work/csharp-app/src/Objects/Store.cs"
long "$work/robin-cli/src/Program.cs"
(cd "$work/csharp-app" && node /path/to/template/scripts/check-file-size.mjs); echo "exit $?"
(cd "$work/robin-cli"  && node /path/to/template/scripts/check-file-size.mjs); echo "exit $?"
```

All three files have 1001 lines. Only `Store.cs` is reported (`obj` is case-sensitive, so `Objects` does not match), `Robin.cs` is skipped, and the `robin-cli` checkout checks nothing:

```
Checking C# files for maximum 1000 lines (warning above 900)...

Found files exceeding the line limit:

  src/Objects/Store.cs: 1001 lines (exceeds 1000)

Please refactor these files to be under 1000 lines

exit 1
Checking C# files for maximum 1000 lines (warning above 900)...

Checked 0 file(s) - all within the line limit

exit 0
```

We hit this in link-foundation/link-cli (issue 134, pull request 135), whose copy also checks Rust: `rust/examples/lino_binary_dump.rs` (`bin`) and `rust/src/sequences/target_matcher.rs` (`target`) were never measured. The script and log are in that pull request under `dev/log/issues/134/pulls/135/scripts/template-file-size-substring-exclusion.sh` and `dev/log/issues/134/pulls/135/analysis/template-file-size-repro.txt`.

## Workaround

Avoid the excluded words in file and checkout directory names.

## Suggested fix

Compare whole path components relative to the scanned root:

```js
import { relative, sep } from 'node:path';

export function shouldExclude(path, root) {
  return relative(root, path)
    .split(sep)
    .some((component) => EXCLUDE_PATTERNS.includes(component));
}
```

and a test that `src/Robin.cs` and `.github/scripts/Tool.cs` are reported while `src/App/bin/Debug/X.cs` and `src/App/obj/X.cs` are not. link-cli's version: `csharp/scripts/check-file-size.mjs` and `csharp/scripts/check-file-size.test.mjs`.
