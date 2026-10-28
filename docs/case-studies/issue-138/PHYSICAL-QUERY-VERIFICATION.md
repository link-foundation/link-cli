# Query-driven physical session recipes

The existing Rust, C# and actual-WASM JavaScript examples now stage standard
create/name operations, match rows through original-text query envelopes, match
the complete encoded `initial` name including BOM/Unicode/NUL, delete through a
bound-reference query, and publish with the original explicit tokens. The
independent expected complete versions, causal counters, generations and exact
receipt retries are unchanged. In particular, dangling reference 3 is matched
through variables; no extra row changes the persisted fixture.

Each nonempty generation hashes or captures every provider file before staging,
checks owned query observations, prepares the exact original request, mutates
the returned request copy and verifies that the retained request is unchanged.
Edits/queries through retained adapters reject after preparation. Staging and
preparation preserve every file byte. The existing lost-reply recovery retries
the pre-exit request, and stale publication still preserves physical bytes.
These assertions extend the existing executable integration contract; the raw
row/name recipes in the preceding commits remain independent positive controls.

Focused local checks on 2026-10-28:

```sh
export DOTNET_PROCESSOR_COUNT=2 DOTNET_GCHeapHardLimit=0x40000000
dotnet build examples/cluster-csharp --configuration Release -m:1 -p:UseSharedCompilation=false
NATIVE_LSM_LAYOUT=session NATIVE_LSM_RUNTIMES=csharp python3 examples/native-lsm-js/run-interop.py
NATIVE_CAUSAL_LAYOUT=session NATIVE_LSM_RUNTIMES=csharp python3 examples/native-lsm-js/run-causal-faults.py
cargo fmt --manifest-path rust/Cargo.toml -- --check
node --check examples/native-lsm-js/session-interop.mjs
```

The build passes without warnings. Both C# layouts pass two handoffs, four
finite reaped exits, four unchanged-byte blocked opens and four complete
retry/reopen/following sequences. Fault children additionally have the harness's
8 MiB stack and 640 MiB managed heap bounds. Full suites and full five-runtime
builds/matrices are reserved for CI following the latest feedback; the existing
JS client integration job runs all fifty handoffs, twenty reaped exits, one
hundred blocked opens and one hundred recovery sequences. Those complete
results pass on `cbddc1343bbc07a1d5b8b52d2772c7749c18fb89` in
[JS client CI run 40684594403](https://github.com/link-foundation/link-cli/actions/runs/40684594403),
created 2026-10-28T12:02:27Z. Its raw log lines 5673 and 6284 report the session
handoff and exit/retry totals; all 100 blocked opens also pass unchanged-byte
checks. Rust/C#/Node/Bun/Deno consume both plain and mirrored stores. The
[retained CI receipt](../../../dev/log/issues/138/pulls/139/physical-query-ci.txt)
records source-log hashes, exact result lines and platform coverage. This
predates the subsequent allocator fixes and does not verify those commits.

The local recipe logs are preserved under `dev/log/issues/138/pulls/139/` with
the `physical-query-csharp` prefix. Application process exits do not establish
hardware power-loss safety. All wider unmet R1–R12 requirements remain active.
