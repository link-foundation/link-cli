# Contributing to link-cli

link-cli has three implementations that must stay at parity:

- C# in `csharp/`
- Rust in `rust/`
- JavaScript in `js/`, which holds the browser workbench and the client
  library

This guide covers the rules every change follows, whether a person or an AI
agent writes it. Agents also follow [AGENTS.md](AGENTS.md).

## Before you open a pull request

1. Branch from `main`. Nobody pushes to `main` directly: branch protection
   requires the three stage checks listed below.
2. Install the JavaScript tools once: `npm ci --prefix js`.
3. Run the JavaScript stage locally. Every pull request runs it first in CI.

   ```bash
   npm --prefix js run lint            # ESLint + Prettier through check-javascript-quality.mjs
   npm --prefix js run format          # auto-fix formatting
   npm --prefix js run test:js         # workbench, CI-script and parity unit tests
   node parity/check-parity.mjs --base origin/main
   ```

4. Run the tests for the stack you changed:
   - Rust: `cargo test --all-features` in `rust/`
   - C#: `dotnet test` in `csharp/`
   - client library: `npm test` in `js/client/`
5. When a feature's code changed, run `node parity/check-parity.mjs --update`
   and commit the refreshed fingerprints. If you changed the feature in only
   one language on purpose, acknowledge it with `--acknowledge`. See
   [docs/PARITY.md](docs/PARITY.md).
6. Add a release note for each package you changed:
   - Rust: a fragment in `rust/changelog.d/` (see its README)
   - C#: a changeset in `csharp/.changeset/` (see its README)
7. Optional: install pre-commit (`pip install pre-commit && pre-commit
   install`) to run the hooks in `.pre-commit-config.yaml` on each commit.

## CI stages

All three stage checks are required on `main`:

| Order | Required check                 | Workflow                         | Runs                                                                                     |
| ----- | ------------------------------ | -------------------------------- | ---------------------------------------------------------------------------------------- |
| 1     | `Pipeline Status (javascript)` | `javascript.yml`                 | `npm ci`, lint, `npm run test:js` and the parity checker. It needs no Rust or .NET.      |
| 2     | `Pipeline Status (rust)`       | `rust.yml`                       | The Rust jobs, which start only after the JavaScript stage passed for the same commit.   |
| 2     | `Pipeline Status (csharp)`     | `csharp.yml`                     | The C# jobs, likewise.                                                                   |

Every other workflow that installs Rust or .NET calls the same reusable
JavaScript stage and waits for it:

- `wasm.yml`
- `crash.yml`
- `ipc.yml`
- `deep-client.yml`
- `js-client.yml`
- `docs.yml`
- `security.yml`
- `dependencies.yml`

`.github/scripts/workflow-policy.test.mjs` fails if a job can start Rust or C#
tooling without that gate. So a pull request with a failing `npm run test:js`
starts no Rust or C# job.

`rust.yml` and `csharp.yml` run on every pull request, because a path filter
would skip the parity check and leave a required check pending. When a pull
request does not touch a stack, `.github/scripts/stack-paths.mjs` skips that
stack's build and test jobs, and its status check still reports.

When CI fails, start with
[docs/ci-cd/troubleshooting.md](docs/ci-cd/troubleshooting.md).

## Pushing fixes for CI failures: the single-pusher rule

Each push to a pull request branch cancels the runs in progress and starts
every workflow again, so racing pushes waste the CI budget. A green result
from a superseded commit also proves nothing. So:

1. **One pusher per branch.** When several people or agents work on one pull
   request, exactly one of them pushes, and the others hand their changes to
   that pusher.
2. **Never push while a run is in progress on the same branch.** Check first:

   ```bash
   gh run list --branch <branch> --json status \
     --jq '[.[] | select(.status != "completed")] | length'   # must print 0
   ```

3. **Push only after every failure of the last completed run is fixed.**
   Download the logs of every failed job (`gh run view <id> --log-failed`),
   fix all the causes locally, verify each fix, then push once. Do not push
   one fix per failure.
4. Infrastructure failures are rerun automatically by
   `rerun-infrastructure-failures.yml`; do not push to retry them. This covers
   a runner that was never acquired, as opposed to a test that failed.

## Pull request hygiene

- Each pull request closes an issue: write `Fixes #<n>` in its description.
- Keep pull requests green. A red pull request cannot merge, because of
  branch protection.
- **Stale pull requests.** `stale.yml` labels a pull request `stale` after 60
  days without activity and closes it 14 days later. Any update, or removing
  the label, resets the clock. Add the `keep-open` label to exempt one. Issues
  never go stale: they track work that is still wanted.
- Dependency updates come from Dependabot, and `dependencies.yml` keeps every
  dependency on its latest release. A dependency that cannot be updated yet
  carries a comment linking the open issue that blocks it. When the check
  fails on `main`, it opens or updates a tracking issue.

## Code review

`.github/CODEOWNERS` requests a review from the maintainers on every pull
request.
