# Ordinary query/protocol calls in physical session interchange

Continue the C# checked-query and protocol contracts before claiming this matrix.
The existing `session` matrix transfers complete plain/mirrored native LSM stores
between Rust, C#, Node, Bun and Deno. It also reaps finite exits before and after
publication and verifies unchanged physical hashes on blocked/stale opens. Those
raw row/name operations remain historical evidence; they do not verify queries.

- [ ] Add an independent failing physical query contract using the existing
      session recipes and unchanged expected complete-store versions/receipts.
      Keep publication tokens 100/200/300/400 and their exact original requests.
- [ ] Replace initial raw create/name calls with the standard selected-address
      protocol operations. Through the original-text query envelope, match the
      resulting rows and their full stored names without publishing a query save.
      Existing seed rows have dangling reference 3, so use bound variables for
      these references; do not silently create an extra published row to satisfy
      numeric query reference validation.
- [ ] Stage the second generation's deletion through the ordinary query path
      and rename through standard protocol operations. Assert owned changes and
      private rows, and compare all provider files/values before explicit commit.
      Keep the current exact expected versions and counter ordering.
- [ ] Refuse operations on a prepared client, retain the exact request, then
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
