## Summary

`scripts/version-and-commit.mjs` and `scripts/merge-changesets.mjs` **skip a changeset they cannot parse** (for example `'MyPackage': minr`), printing only `Warning: Could not parse version type from …`. The release then deletes *every* changeset file, including the skipped one, and ends green. The broken changeset's bump and its description are silently lost: a minor change is released as a patch and is missing from `CHANGELOG.md` and the GitHub release.

`scripts/validate-changeset.mjs` does not prevent this, because it only checks changesets that a PR **adds** (`status === 'A'`). A PR that edits a pending changeset, a direct push, or a release-PR branch (which skips the check) brings an unparseable changeset to `main` unchecked.

Code (template `main` at 4cc372e):

- [`processChangesets`](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/4cc372ebf2075a0c948b46881143f7d66f0f0e7e/scripts/version-and-commit.mjs#L253-L292): `if (parsed) { parsedChangesets.push(…) }`; any failure only warns
- [`removeChangesetFiles`](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/4cc372ebf2075a0c948b46881143f7d66f0f0e7e/scripts/version-and-commit.mjs#L346-L353): deletes every changeset, parsed or not
- [`merge-changesets.mjs` `main`](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/4cc372ebf2075a0c948b46881143f7d66f0f0e7e/scripts/merge-changesets.mjs#L208-L223): fails only when *no* changeset parses
- [`parseAddedChangesets`](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/4cc372ebf2075a0c948b46881143f7d66f0f0e7e/scripts/validate-changeset.mjs#L53-L70): only `A` entries are validated

## Reproduction

`repro-csharp-invalid-changeset.sh` clones the given revision with a bare `origin`. It replaces the pending changesets with a valid patch changeset and a minor one whose bump type has a typo, then runs `node scripts/version-and-commit.mjs --mode changeset` as the release job does:

```bash
repro-csharp-invalid-changeset.sh /path/to/template-clone HEAD
```

Output against 4cc372e (abridged):

```
Warning: Could not parse version type from .changeset/add-export.md
Bump types found: patch
Using highest: patch
...
Output: version_committed=true
Output: new_version=0.3.17
exit=0
== changesets left in .changeset
README.md
config.json
== newest CHANGELOG.md entry
## [0.3.17] - 2026-10-24

Fix a typo in the help text.

== does CHANGELOG.md mention the minor change?
0
```

The script needs only `git` and `node`. It lives in link-foundation/link-cli (issue 134, pull request 135) at `dev/log/issues/134/pulls/135/scripts/repro-csharp-invalid-changeset.sh`. The outputs are `analysis/template-csharp-invalid-changeset-repro.txt` (template) and `analysis/csharp-invalid-changeset-repro.txt` (link-cli before and after the fix).

## Workaround

Before merging, check every pending changeset by hand with `node scripts/validate-changeset.mjs` (outside a PR it validates all of them). After a release that went wrong, restore the deleted changeset from git and release the missing bump manually.

## Suggested fix

Fail instead of skipping, in both scripts:

```js
const parsedChangesets = [];
const invalid = [];
for (const file of files) {
  const filePath = join(CHANGESET_DIR, file);
  const parsed = parseChangeset(filePath);
  if (parsed) {
    parsedChangesets.push({ file, filePath, ...parsed });
  } else {
    invalid.push(file);
  }
}

// Every changeset is deleted after the release, so one that does not parse
// would lose its bump and its description on a green run.
if (invalid.length > 0) {
  console.error(
    `::error title=Invalid changeset::Cannot parse ${invalid.join(', ')} in ${CHANGESET_DIR}. ` +
      `How to fix: start it with ---, '${PACKAGE_NAME}': major|minor|patch, --- and a description.`
  );
  process.exit(1);
}
```

Tests worth adding (link-cli has both in `csharp/scripts/release-scripts.test.mjs`):

- `merge-changesets` exits 1 with `Invalid changeset` and keeps every file;
- `version-and-commit --mode changeset`, run against a bare remote, exits 1 and pushes neither a commit nor a tag.

Optionally, `validate-changeset.mjs` could also validate changesets a PR modifies (`M`), so that the error shows up on the PR instead of on `main`.

The js template is not affected: its `merge-changesets.mjs` leaves an unparseable file in place, and `changeset version` (`@changesets/parse`) rejects it with `invalid version type`.
