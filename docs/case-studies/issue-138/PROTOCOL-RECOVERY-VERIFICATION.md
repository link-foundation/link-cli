# Recovered C# protocol validation

On 2026-10-28, recovery commit `2dff50de` was fast-forwarded onto the prepared
branch. It preserves the interrupted draft rather than reconstructing it from
the successful intermediate log. The final recovered source passes 144 focused
Release tests: causal queries, selected-address operations, codec framing,
existing server and existing remote-store contracts. There are no skipped tests.

Reproduce with `bash experiments/issue-138/run-csharp-causal-protocol.sh`.
The local SDK is 10.0.112; build concurrency is one, processor count two, shared
compilation is disabled and the managed heap is capped at 1 GiB. Complete
Debug/Release, platform and coverage suites are reserved for CI, following the
[maintainer's instruction](https://github.com/link-foundation/link-cli/pull/139#issuecomment-6467849231).

The restored gate referenced two scripts absent from the recovery commit; both
are now supplied. ShellCheck can be selected through `SHELLCHECK`, avoiding an
untracked machine-specific tools path. The cheap gate passes 221 repository
tests, 36 release-helper tests, source-size and changeset validation, ShellCheck
and whitespace checks. Its first local run found an empty root `ci-logs`
directory; removing that empty investigation directory resolves the layout test.

Formatting verification passes when scoped to all 13 restored C# files. The
older local formatter also proposes unrelated indentation changes to established
files, so those edits were discarded; CI's pinned 10.0.403 SDK owns complete
repository formatting validation. The operation's address-admission loop now
uses braces to make its indentation consistent between formatter versions.

Raw focused and gate logs are retained in `dev/log/issues/138/pulls/139/` with
the `csharp-protocol-recovered` prefix. The final recovered byte-framing and
maximum-address tests are included in the 144-test result; the earlier 219-test
log is historical evidence only. Published head `08345d9` had nine passing
workflows, all created at 2026-10-28T00:33:58Z. Those runs do not verify this
recovery commit. Exact-head CI remains required after pushing.

This step completes the selected-address C# protocol draft. Physical query
interchange, complete coverage and the other open R1–R12 criteria remain active.
