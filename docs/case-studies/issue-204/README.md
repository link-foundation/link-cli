# Issue 204 – Dependencies check failing on main

The scheduled Dependencies audit ran against `66f49e8c` on November 27, 2026. It
found six outdated entries after checking 184 declared versions and the tracked
Cargo lockfiles. [Scheduled audit](https://github.com/link-foundation/link-cli/actions/runs/46019688128/job/140576033187)

| Dependency | Version in the audit | Latest at audit time | Entries |
| --- | --- | --- | --- |
| `lucide-react` | `1.81.0` | `1.82.0` | Workbench manifest and lockfile |
| `cc` | `1.11.3` | `1.12.0` | Rust CLI and WebAssembly lockfiles |
| `find-msvc-tools` | `0.1.15` | `0.1.16` | Rust CLI and WebAssembly lockfiles |

Commit `3eaf9617` brought all three updates into `main`. The push audit of
`d3ef0b92` reported two remaining outdated entries for ESLint `10.15.0`.
It ran in report mode, so those entries appeared as a warning. A local
enforcement audit reproduced both entries and exited with status 1.
[PR #203](https://github.com/link-foundation/link-cli/pull/203) · [Push audit](https://github.com/link-foundation/link-cli/actions/runs/46033115131)

PR #206 updates the workbench manifest and npm lockfile to ESLint `10.16.0`.
It also removes the root `.gitkeep` created during setup. That file made the
repository-layout test fail in the shared JavaScript stage of every initial PR
workflow, which prevented the Dependencies job from running.
[PR #206](https://github.com/link-foundation/link-cli/pull/206) · [Initial JavaScript run](https://github.com/link-foundation/link-cli/actions/runs/46043311415/job/140656223833)

Run the audit from the repository root on `301e298f` and on the fixed PR head
to reproduce and verify the dependency update. The audit compares against
live registries, so a later release can change the expected output.

```sh
CHECK_DEPENDENCIES_MODE=enforce node .github/scripts/check-latest-dependencies.mjs
```

The existing layout test reproduces the `.gitkeep` failure and verifies its
removal.

```sh
node --test --test-name-pattern='generated evidence stay out' js/test/repositoryLayout.test.mjs
```
