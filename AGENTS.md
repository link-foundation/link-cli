# Rules for AI agents working on link-cli

Read [CONTRIBUTING.md](CONTRIBUTING.md) first; everything in it applies to
agents. This file adds the rules that agents in particular have broken
before.

## The single-pusher rule for CI/CD fixes

1. **Exactly one agent pushes to a branch.** Sub-agents and parallel workers
   never push. They return their patch or commit to the coordinating agent,
   which is the only one that pushes.
2. **Never push while a run is in progress on the same branch.** Before every
   push, run this and wait until it prints `0`:

   ```bash
   gh run list --branch "$(git branch --show-current)" --json status \
     --jq '[.[] | select(.status != "completed")] | length'
   ```

3. **Push only after every failure of the last run is fixed.** For each
   failed job:
   - save the log (`gh run view <run-id> --log-failed > ci-logs/<name>-<run-id>.log`)
   - find the root cause
   - fix it, and verify the fix locally

   Then push once. Do not push to "see if CI passes", and do not push one fix
   per failure.
4. Do not push only to retry an infrastructure failure, such as a runner that
   was never acquired. `rerun-infrastructure-failures.yml` reruns those.

## Stage order and parity

- The JavaScript stage runs first: lint, `npm run test:js` and
  `node parity/check-parity.mjs`. When it fails, no Rust or C# job runs, so
  fix it first.
- Any new workflow job that installs or runs Rust (`cargo`, `rustup`,
  `wasm-pack`) or .NET (`dotnet`, `actions/setup-dotnet`) must:
  - list `javascript` in its `needs:`, where the `javascript` job calls
    `./.github/workflows/javascript-stage.yml`;
  - and, if its `if:` uses `always()` or `!cancelled()`, also require
    `needs.javascript.result == 'success'`.

  `workflow-policy.test.mjs` enforces this.
- When you change a feature in one implementation, either:
  - change the others in the same pull request, or
  - record a reasoned `--acknowledge` in `parity/features.json`.

  Never edit fingerprints by hand; run `node parity/check-parity.mjs --update`.
  See [docs/PARITY.md](docs/PARITY.md).
- Do not weaken a check to make CI pass. That covers path filters on
  `rust.yml`, `csharp.yml` and `javascript.yml`, skipping the golden fixture
  test, and disabling a policy test. Fix the cause instead.

## Working conventions

- Keep experiments in `experiments/` and runnable examples in `examples/`.
- Every source file stays at or below 1000 lines. The linters warn from 900.
- Add a `rust/changelog.d/` fragment or a `csharp/.changeset/` file for each
  package you change.
- Case studies of non-trivial issues go in `docs/case-studies/issue-<n>/`.
