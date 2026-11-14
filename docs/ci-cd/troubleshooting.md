# CI/CD troubleshooting

This page explains how to read a red check on a link-cli pull request and
how to get more output from each tool. Before you push a fix, read the
single-pusher rule in [CONTRIBUTING.md](../../CONTRIBUTING.md).

## Stages

CI runs in two stages. The JavaScript stage must succeed before any Rust or
C# job starts. Since issue #111, branch protection on `main` requires these
three checks:

| Stage | Required check                 | Entry workflow   | What fails it                                                                 |
| ----- | ------------------------------ | ---------------- | ----------------------------------------------------------------------------- |
| 1     | `Pipeline Status (javascript)` | `javascript.yml` | ESLint or Prettier, `npm run test:js`, or the parity checker.                 |
| 2     | `Pipeline Status (rust)`       | `rust.yml`       | Formatting, clippy, Rust tests, the golden fixture receipt, or stage 1.      |
| 2     | `Pipeline Status (csharp)`     | `csharp.yml`     | `dotnet format`, build, C# tests, the golden fixture receipt, or stage 1.     |

Stage 1 is the reusable workflow `.github/workflows/javascript-stage.yml`,
which shows up as the "JavaScript stage" job. Every workflow that builds or
runs Rust or .NET calls it as its `javascript` job and lists it in `needs:`:

- `rust.yml`
- `csharp.yml`
- `wasm.yml`
- `crash.yml`
- `ipc.yml`
- `deep-client.yml`
- `js-client.yml`
- `docs.yml`
- `security.yml`
- `dependencies.yml`

### "Every Rust and C# job was skipped"

That is the gate working. Open the `javascript / JavaScript stage` job of any
of those workflows and fix the first failing step there. Each workflow runs
the stage again, so it fails the same way in all of them. Fix it once.

### "A job runs Rust or .NET without waiting for the JavaScript stage"

`Workflow Policy Tests` (in `workflows.yml`) fails with a list of `file: job`
pairs. Make that job depend on the stage:

- Add `javascript` to the job's `needs:`, where `javascript` is a job that
  calls `./.github/workflows/javascript-stage.yml`.
- If the job's `if:` uses `always()` or `!cancelled()`, add
  `needs.javascript.result == 'success'` to it.

A job that only needs another gated job is already covered.

### "A required check is pending forever"

Required checks need workflows that run on every pull request. Do not add
`paths:` or `paths-ignore:` to `javascript.yml`, `rust.yml` or `csharp.yml`;
`ci-governance.test.mjs` rejects that. When a stack is unchanged,
`stack-paths.mjs` skips its build jobs inside the workflow instead. To see its
decision for each file, run the step with debug logging (see below) or
locally:

```bash
STACK_PATHS_DEBUG=1 GITHUB_EVENT_NAME=pull_request GITHUB_BASE_SHA="$(git merge-base origin/main HEAD)" \
  GITHUB_HEAD_SHA=HEAD node .github/scripts/stack-paths.mjs rust
```

## Parity failures

The `Check cross-language parity` step prints one finding per line. The
checker's rules and fixes are in [PARITY.md](../PARITY.md#what-the-checker-rejects).
The common ones:

| Finding                                                    | Fix                                                                                                    |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `fingerprint is stale`                                     | Run `node parity/check-parity.mjs --update`, then commit `parity/features.json`.                      |
| `<id>: only rust changed; csharp did not`                  | Port the change to the other languages, or acknowledge it with `--acknowledge <id> --reason --ref`.   |
| `belongs to no feature`                                    | Add the new file to a feature's `sources`/`tests`, or to `shared` with a reason.                      |
| `matches no tracked file`                                  | A file was renamed or deleted. Update the glob.                                                        |
| `base commit … is not in this clone`                       | The checkout is shallow. The stage needs `fetch-depth: 0`, so do not change it.                        |

To print every matched file and fingerprint:

```bash
node parity/check-parity.mjs --base origin/main --verbose
```

`PARITY_DEBUG=1` has the same effect, and CI sets it when debug logging is on.

## Golden fixture receipts

The "Check that the Rust/C# suite loaded every golden fixture" step in
`rust.yml` and `csharp.yml` fails in two cases:

- the receipt file is missing: the golden fixture test did not run, perhaps
  because a filter excluded it, it was skipped, or the build failed first;
- the receipt lists fewer fixtures, or other hashes, than
  `parity/golden-fixtures.json`.

Run the test locally with a receipt:

```bash
GOLDEN_FIXTURES_RECEIPT=/tmp/rust-receipt.json \
  cargo test --manifest-path rust/Cargo.toml --test golden_fixtures_tests
node parity/check-parity.mjs --receipt rust /tmp/rust-receipt.json
```

## Workflow lint: actionlint and zizmor

`workflows.yml` runs both linters. To run them locally from the repository
root:

```bash
docker run --rm -v "$PWD:/repo" -w /repo rhysd/actionlint:1.7.12 -color
docker run --rm -v "$PWD:/repo:ro" -w /repo ghcr.io/zizmorcore/zizmor:1.32.0 \
  --offline --config .github/zizmor.yml .github
```

actionlint runs inside its Docker image, which bundles shellcheck, so it also
lints every `run:` script.

zizmor's configuration is `.github/zizmor.yml`, and each disabled audit there
has a written reason. In particular, `self-repository` is disabled:

- zizmor 1.32 asks for `uses: $/.github/workflows/...` instead of
  `uses: ./.github/workflows/...`.
- actionlint 1.7.12 rejects the `$/` form (rhysd/actionlint#711).

To keep the audit's purpose, `ci-governance.test.mjs` forbids step-level
`uses: ./` local actions. Re-enable the audit when actionlint accepts `$/`.

## Other linters

| Linter            | Where                                  | Local command                                   |
| ----------------- | -------------------------------------- | ----------------------------------------------- |
| ESLint + Prettier | `js/` (`check-javascript-quality.mjs`) | `npm --prefix js run lint` / `npm --prefix js run format` |
| rustfmt + clippy  | `rust.yml`                             | `cargo fmt --check`, `cargo clippy --all-targets --all-features` in `rust/` |
| dotnet format     | `csharp.yml`                           | `dotnet format --verify-no-changes` in `csharp/` |
| CodeQL            | `security.yml` (`.github/codeql/codeql-config.yml`) | none; read the code scanning alerts            |
| pre-commit hooks  | `.pre-commit-config.yaml`              | `pip install pre-commit && pre-commit run --all-files` |

The JavaScript quality check only sees files that git tracks, so `git add` a
new file before you lint it. It warns from 900 lines and fails above 1000.

## Dependencies

`dependencies.yml` fails when a dependency is behind its latest release. A
pin that carries `# held back: <issue URL>` counts as held back while that
issue is open. Once the issue closes, the pin fails the check again, so it
cannot be forgotten. Example: cargo-audit 0.22.2 was held back by #176 until
`taiki-e/install-action` v2.90.4 shipped its 0.23.0 manifest.

When the check fails on `main`, the `tracking-issue` job opens an issue titled
"Dependencies check is failing on main", or comments on the open one, and
closes it after the next green run. To preview its decision without writing
anything:

```bash
NEEDS_JSON='{"latest":{"result":"failure"}}' TRACKING_ISSUE_DRY_RUN=1 \
  TRACKING_ISSUE_DEBUG=1 GITHUB_REPOSITORY=link-foundation/link-cli \
  GITHUB_TOKEN="$(gh auth token)" node .github/scripts/dependency-tracking-issue.mjs
```

## Stale pull requests

`stale.yml` runs daily. To see what it would label or close without changing
anything, run it with the `debug_only` input:

```bash
gh workflow run stale.yml -f debug_only=true
```

## Debug switches

| Switch                                  | Effect                                                                         |
| --------------------------------------- | ------------------------------------------------------------------------------ |
| Re-run jobs with "Enable debug logging" | Sets `runner.debug`, which turns on every switch below in CI.                  |
| `PARITY_DEBUG=1` or `--verbose`         | The parity checker prints each matched file and fingerprint.                   |
| `STACK_PATHS_DEBUG=1`                   | `stack-paths.mjs` prints each changed file and which stack it counts for.      |
| `TRACKING_ISSUE_DEBUG=1`                | `dependency-tracking-issue.mjs` logs each GitHub API request.                  |
| `TRACKING_ISSUE_DRY_RUN=1`              | `dependency-tracking-issue.mjs` decides but writes nothing.                    |
| `debug_only` input of `stale.yml`       | actions/stale reports without labelling or closing anything.                   |
