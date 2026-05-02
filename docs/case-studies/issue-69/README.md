# Issue 69: GitHub Pages CI/CD Failure

## Summary

The failed WebAssembly CI run was caused by the `Deploy GitHub Pages` job calling
`actions/configure-pages@v5` on every push to `main` before GitHub Pages had been
enabled for the repository and configured to use GitHub Actions.

The build and tests passed. The failure happened only when the workflow tried to
read repository Pages configuration:

```text
Get Pages site failed. Please verify that the repository has Pages enabled and
configured to build using GitHub Actions
```

## Evidence

- Workflow run: <https://github.com/link-foundation/link-cli/actions/runs/25245349330>
- Run metadata: `docs/case-studies/issue-69/evidence/run-25245349330.json`
- Full log: `docs/case-studies/issue-69/evidence/run-25245349330.log`
- Recent runs for `issue-69-43fc7f1a4ec3`: `docs/case-studies/issue-69/evidence/recent-runs-issue-branch.json`
- Fixed push run metadata: `docs/case-studies/issue-69/evidence/run-25249954218.json`
- Fixed pull request run metadata: `docs/case-studies/issue-69/evidence/run-25249954818.json`

## Timeline

- 2026-05-02 06:01:29 UTC: WebAssembly CI started on `main` after merge commit
  `26050750382b801a4d9f33d1445627e2db2b7867`.
- 2026-05-02 06:04:27 UTC: `Test` job completed successfully after Rust tests,
  WebAssembly tests, React build, and artifact upload.
- 2026-05-02 06:04:29 UTC: `Deploy GitHub Pages` job started.
- 2026-05-02 06:06:28 UTC: Pages build completed successfully.
- 2026-05-02 06:06:29 UTC: `Configure Pages` failed because the Pages site API
  returned `404 Not Found`.

## Root Cause

`wasm.yml` deployed to Pages automatically on every push to `main`:

```yaml
if: github.event_name == 'push' && github.ref == 'refs/heads/main'
```

That assumes repository Pages configuration already exists. For this repository,
the Pages site was not enabled/configured for GitHub Actions at the time of the
run, so `actions/configure-pages@v5` failed before artifact upload or deployment.

## Requirements

- Preserve the CI logs and metadata under `docs/case-studies/issue-69`.
- Keep WebAssembly tests and production app builds running automatically.
- Prevent unconfigured Pages deployment from failing normal `main` pushes.
- Preserve a way to deploy Pages once the repository owner enables Pages.
- Compare with CI/CD template practices and document the applicable findings.

## Template Comparison

The referenced JS, Rust, and C# templates consistently keep CI validation
separate from release/deploy operations and use explicit conditions for jobs
with side effects. The Rust template case-study data also shows a documentation
Pages deployment guarded separately from package release work.

No referenced template required `actions/configure-pages` on every normal push
for a repository that may not have Pages enabled. The applicable best practice is
to keep normal CI deterministic and make environment-dependent deployment an
explicitly gated action.

## Fix

The WebAssembly workflow now keeps the test/build/artifact job on push and pull
request events, but gates the `Deploy GitHub Pages` job behind manual
`workflow_dispatch` with `deploy_pages: true` on `main`.

This removes the failing Pages API call from normal `main` pushes while keeping
the existing GitHub Pages deployment steps available after repository Pages is
enabled and configured.

## Verification

- `node experiments/validate-wasm-workflow.mjs`
- `npm run build`
- Push WebAssembly CI run `25249954218`: `Test` succeeded and `Deploy GitHub Pages` skipped.
- Pull request WebAssembly CI run `25249954818`: `Test` succeeded and `Deploy GitHub Pages` skipped.
