# Issue 201 – Dependencies check failing on main

The [scheduled Dependencies audit](https://github.com/link-foundation/link-cli/actions/runs/45843672479)
ran at 14:54 UTC on November 26, 2026 against `2bbd35da`. The JavaScript stage
passed. The enforcement job found four stale entries in its registry comparison
(lines 1061–1072 of the workflow log).

| Dependency | Declared and locked | Registry at audit time |
| --- | --- | --- |
| `lucide-react` | `1.80.0` | `1.81.0` |
| `vite` | `8.3.8` | `8.4.0` |

The four entries represent two dependencies, each reported once for the
workbench manifest and once for its lockfile. Registry lookups succeeded, and
neither update needed a holdback. The tracking job opened
[issue #201](https://github.com/link-foundation/link-cli/issues/201) at 14:55 UTC.
A later release of [install-action v2.90.11](https://github.com/taiki-e/install-action/releases/tag/v2.90.11)
at 17:18 UTC made two workflow pins stale as well. The full local audit before
the fix reported all six entries.

The [first PR audit](https://github.com/link-foundation/link-cli/actions/runs/45900520618)
stopped before comparing dependencies. The JavaScript layout test rejected the
root `.gitkeep` created during PR setup (lines 858–860). The same assertion failed
in the other four workflows because they share the JavaScript stage.

[PR #202](https://github.com/link-foundation/link-cli/pull/202) updates both
workbench ranges, regenerates the npm lockfile, pins both workflows to the commit
tagged `v2.90.11`, and removes the setup placeholder. GitHub Pages deploys the
private workbench on a qualifying push to `main`.

To reproduce the failure against the affected revision and verify the fix, run
the same audit in enforcement mode from the repository root. It compares the
checked-out versions with the live registries, so later releases may change the
expected output.

```sh
node .github/scripts/check-latest-dependencies.mjs
```

The separate layout regression check rejects a root `.gitkeep` directly.

```sh
node --test --test-name-pattern='generated evidence stay out' js/test/repositoryLayout.test.mjs
```
