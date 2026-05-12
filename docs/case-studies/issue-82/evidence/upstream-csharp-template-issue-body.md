The template release script can report `already_released=true` for a version whose git tag does not exist.

## Reproducer

1. Start from a repository using `scripts/version-and-commit.mjs`.
2. Keep the package version at `2.3.0`.
3. Add a minor changeset so the next version is `2.4.0`.
4. Ensure `v2.4.0` is not present:

```sh
git rev-parse --verify --quiet refs/tags/v2.4.0
echo $?
```

5. Run:

```sh
bun run scripts/version-and-commit.mjs --mode changeset
```

## Actual result

The script can print that the tag already exists and exit with:

```text
already_released=true
new_version=2.4.0
```

No version bump, changelog update, release commit, or tag is created.

## Root cause

`exec(command, true)` catches command failures and returns an empty string. `checkTagExists()` wraps the silent command in `try/catch`, but the catch block is never reached when `git rev-parse` fails. As a result, a missing tag is treated as existing.

The same wrapper also affects `git diff --cached --quiet`: staged changes can be treated as no changes because the nonzero exit code is swallowed.

## Workaround

Manually verify whether the tag exists with `git rev-parse --verify --quiet refs/tags/vX.Y.Z` before trusting `already_released=true`, then rerun with a patched script if the tag is missing.

## Suggested fix

Make the command wrapper throw for failed commands even in silent mode, and verify the exact tag ref:

```js
function exec(command, silent = false) {
  return execSync(command, { encoding: 'utf-8', stdio: silent ? 'pipe' : 'inherit' });
}

function checkTagExists(version) {
  try {
    exec(`git rev-parse --verify --quiet refs/tags/v${version}`, true);
    return true;
  } catch {
    return false;
  }
}
```

Add a regression test that initializes a temporary git repository, creates a changeset for a missing next tag, runs `version-and-commit.mjs`, and asserts the version commit and tag are created.
