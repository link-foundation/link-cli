# Ordinary query/protocol calls in physical session interchange

The C# checked-query and selected-address protocol contracts are implemented.
The existing `session` matrix transfers complete plain/mirrored native LSM stores
between Rust, C#, Node, Bun and Deno. It also reaps finite exits before and after
publication and verifies unchanged physical hashes on blocked/stale opens. Those
raw row/name results remain historical evidence; fresh CI must verify queries.

- [x] Extend the independent physical contract using the existing session
      recipes and unchanged expected complete-store versions/receipts.
      Keep publication tokens 100/200/300/400 and their exact original requests.
- [x] Replace initial raw create/name calls with the standard selected-address
      protocol operations. Through the original-text query envelope, match the
      resulting rows and their full stored names without publishing a query save.
      Existing seed rows have dangling reference 3, so use bound variables for
      these references; do not silently create an extra published row to satisfy
      numeric query reference validation.
- [x] Stage the second generation's deletion through the ordinary query path
      and rename through standard protocol operations. Assert owned changes and
      private rows, and compare all provider files/values before explicit commit.
      Keep the current exact expected versions and counter ordering.
- [x] Refuse edits/queries on a prepared client, retain the exact request, then
      exercise before/after-reply exits, stale-session refusal and following edits.
      Recovery must retry the original request rather than rebuild a newer session.
- [ ] Run all fifty complete-store handoffs and twenty finite reaped exits in
      plain/mirrored layouts. Keep one hundred recovered retry/reopen/following
      checks and one hundred unchanged-hash blocked opens. Bound child memory and
      progress; reap children on every parent error as in the existing harness.
- [ ] Run the existing compiler, source and release gates before committing;
      retain reproducible commands, source/report hashes and precise counts.
      This matrix verifies application process exits, not hardware power loss.

Initial source audit: `rust/examples/native_lsm/session.rs`,
`examples/cluster-csharp/CausalSessionInterop.cs`,
`examples/native-lsm-js/session-interop.mjs`, `run-interop.py` and
`run-causal-faults.py`. The causal session's explicit publication and physical
store recovery code should remain shared with the ordinary existing recipes.

Focused local validation on 2026-10-28 passes the C# recipe in both physical
layouts: two handoffs, four finite reaped exits, four blocked opens and four
retry/reopen/following sequences. Every generation checks private provider bytes
and exact original prepared requests; initial queries also match the complete
encoded Unicode/BOM/NUL name. No extra seed identity is added. Rust formatting
and JS syntax pass. The full five-runtime matrix remains a CI requirement;
PHYSICAL-QUERY-VERIFICATION.md records scope, commands and limits. These are
recipe assertions against independent expected versions, not new production
query algorithms. The established raw-operation results are positive controls.
