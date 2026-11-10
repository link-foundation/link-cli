# Issue 140 work plan

Objective: implement and verify every IPC requirement in PR #175, preserving
the existing text/binary protocols and client/server behavior. Never merge the PR.

- [x] Read the complete issue and all issue/PR conversation, inline comments and
  reviews. No comments, images or required sub-issues were present at intake.
- [x] Inspect existing implementations and recent protocol PRs (#160, #155).
- [x] Research native IPC semantics and available reusable libraries; preserve
  sources, requirements, implementation choices and verification evidence.
- [x] Implement optional transport decorators on both ends, with arbitrary
  composition, bounded buffering, lifecycle management and opt-in tracing.
- [x] Cover shared memory, mapped files, Unix sockets, TCP, UDP, anonymous
  pipes, named pipes, ALPC, WM_COPYDATA, mailslots, POSIX message queues,
  signals and disk files; expose an extension contract for similar mechanisms.
- [ ] Add focused automated native cross-process, failure and stacking tests,
  and run real text/binary conversations against both existing servers.
- [x] Document configuration, platform restrictions and runnable examples.
- [ ] Commit atomic changes, merge latest main, push only the prepared branch,
  review the full PR diff and resolve all CI failures on the current commit.
- [ ] Replace WIP title/body, preserve `Fixes #140`, mark PR #175 ready,
  verify clean working tree and report concrete results and any blocked criteria.

Implementation decision: supply an optional, dependency-free Python IPC adapter
package. Its relay decorates the existing TCP boundary of every Rust/C#/JS
client/server, so sessions, authentication, subscriptions, triplets and both
wire formats use the existing implementation. Endpoints are duplex byte
streams; mechanisms that only carry events are composed with a payload layer.
Native endpoints must transport payload through the named mechanism rather
than silently substituting TCP. Additional relay processes are an explicit
configuration choice. Existing commands and packages remain usable alone.

Verification budget: no local full Rust/C# builds; use small Python tests and
CI's provisioned runners for actual server conversations and Windows natives.
Keep every source file below the repository's 1000-line Python limit.

## Verification record

- Local native suite: 14 tests pass, covering ten Linux mechanisms plus
  anonymous pipes, composition, registration and failure regressions. Four
  actual-server integration tests are deliberately supplied a built CLI by CI.
- Local workflow helper/policy suite: 268 tests pass. Ruff checks and formatting
  pass with the repository configuration. The bounded signal experiment
  completes 1000 notifications.
- The first Windows native run passed ALPC, WM_COPYDATA, mailslots, named
  pipes and shared storage. Later CI identified incorrect unpacking of the
  Windows pipe polling result in the new cancellation path; the fix is included
  with the original cross-process and blocked-reader regressions.
- The first C# Windows protocol run passed all native text/binary combinations
  and mixed-stack subscriptions; its anonymous-pipe failure was the same
  polling result bug. System.CommandLine 3.0.0 builds without warnings.
- Final current-head native/protocol jobs and all existing repository checks
  must pass before PR #175 is marked ready. Their authoritative final record
  and run URLs are maintained in the PR description, which can be updated
  after CI without changing the code under test.
