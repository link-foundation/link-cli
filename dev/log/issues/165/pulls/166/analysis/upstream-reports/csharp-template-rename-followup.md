Confirmed additional change-detection false negative in snapshot `88bb71832e04a1a52ddb584f1db0f57b6e966646` while completing https://github.com/link-foundation/link-cli/pull/166:

Reproduction: initialize a repository, commit `src/Program.cs` containing `class Program {}`, then `git mv src/Program.cs examples/Program.cs` and commit. Run `getChangedFiles()` with `GITHUB_EVENT_NAME=push`, then pass its result to `detectChangeOutputs()`.

Observed against the actual snapshotted helper: `files=["examples/Program.cs"]`, `outputs={"any-code-changed":"false"}`. Git rename detection omitted the removed production source path, and the target is an excluded examples folder. The same case now has a child-process regression test in link-cli; the source deletion requires its changeset.

Workaround: disable rename detection for the helper invocation (`git -c diff.renames=false ...`), or set repository `diff.renames=false` before running the helper.

Suggested code fix: add `--no-renames` to every `git diff --name-only` call so deletions and additions remain separate input paths. Test moving source into each excluded folder, ordinary source-to-source renames and genuine examples-only edits. The Rust template already uses this approach. Its existing argv-based, strict Git identity/error handling was also reused in link-cli.

Evidence: `dev/log/issues/165/pulls/166/verification/template-csharp-rename.log` in link-cli PR #166; regression: `csharp/scripts/change-detection.test.mjs`.
