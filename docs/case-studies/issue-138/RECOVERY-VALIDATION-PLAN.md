# Validate journals before replay and retain failed recovery ownership

R1/R7/R8: seven finite public-API cases currently accept contradictory decisions,
conflicting owners or payloads of one sequence, nonpositive sequences and a zero
owner. Every case changes row 11 to 21 and appends recovery metadata rather than
rejecting the journal unchanged. The probe has a ten-second deadline, 512 MiB
address-space limit and 8 MiB stack limit.

- [x] Run and preserve the public-API before probe without production changes.
- [x] Add native, C# and actual-WASM regression tests and run them before a fix.
- [x] Validate all recognized records before replay or replacing live indexes;
  reject semantic contradictions, preserve valid idempotent duplicates, legacy
  parsing compatibility, sequence gaps and retained applied checkpoints.
- [x] Reproduce failures on a reachable wrapper, then retain admission through
  failed read, replay, data flush, marker append and journal flush. Retry must
  complete recovery before following writes or unlogged/version replay.
- [x] Verify repeated failures, following commits, repeated actual reopening,
  default-off tracing and costs charged only to selected transaction wrappers.
- [x] Run full bounded native/C#/actual-WASM/JS suites, coverage, formatting,
  strict lint, types, release and source-size checks; inspect their logs.
- [ ] Review changes, commit/push only the prepared branch, update PR 139 and
  verify every workflow on the exact published commit.

Standalone legacy parsers deliberately retain their accepted lexical forms.
Recovery must compare parsed records rather than require newly canonical text.
Identical records and repeated identical decisions can represent idempotent
retry. Contradictory decisions remain invalid even when retention removed their
payload. Read availability during failed live recovery is separate from write
admission; the legacy wrapper does not supply snapshot reads.

Other legacy mutation/journal windows, names, complete coverage and wider issue
requirements remain unfinished. This audit does not repair an already damaged
journal or establish hardware power-loss guarantees.

Before evidence: native validation fails 17/18 cases (the retained checkpoint
control passes); actual WASM fails the same 17 and three live fault functions.
The expanded C# matrix fails 101/144 cases, while 42 constructor-recovery cases
and the checkpoint control pass. Native and WASM reload ownership each fail.
A follow-up regression against the initial ownership draft proves cleanup can
skip an incomplete reload even when external changes are already checkpointed;
both native and WASM fail before retaining that refresh obligation. Two C#
queued-work contracts also fail before their new recovery guard: after a failed
read, checkpoint markers appear and sized/chunked retention can run. Deterministic
queue scheduling uses a sentinel and finite waits, without sleep-based races.
Incidental fixture compilation errors are excluded from preserved before logs.

Focused after checks pass: 29 native and 29 actual-WASM tests (18 journal
validation, eight interruption/reload functions and three ordering controls),
and 146 C# cases (18 validation, 42 constructor interruptions, 84 live retries
and two queued-checkpoint/retention cases). The native live functions exercise
152 applicable phase/cleanup schedules, plus two initial reload schedules and
ten applied-external-state reload/cleanup schedules. Native/WASM restart checks
load separate durable images; C# cases reopen actual mapped files twice. Full
local suites also pass: 765 native Rust, 431 actual WASM, 1,690 C# per
configuration and 106 JS tests, plus coverage/lint/format/types/release/source-size
checks. See recovery-validation.txt. Fresh published-head CI remains pending.

The first full Debug run passes 1,689/1,690 and exposes an older version-intent
fixture that explicitly expected contradictory decisions to survive transaction
recovery. Align that fixture in both languages with the earlier rejection,
preserving its metadata checks; C# additionally checks both journal files across
two failed actual opens. Its ten focused cases pass. The original local failure
and coverage output remain under experiments/issue-138/recovery-validation-first-*.
Native and actual-WASM fixture checks and the complete rerun pass.

The final assertion audit establishes populated committed history before malformed
input. All three cases per native language fail against archive head 7b03c07
because rejection clears that history. Eighteen validation contracts then pass
again in native Rust, actual WASM and C# Debug/Release with fixed source restored.
Complete-suite coverage predates only this test-precondition strengthening;
production source is identical. See recovery-history-*-before.txt.
