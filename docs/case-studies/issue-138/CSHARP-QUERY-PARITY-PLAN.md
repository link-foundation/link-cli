# C# standard query path over private sessions

Continue CAUSAL-PROTOCOL-PLAN.md. The C# processor now shares selected-address
matching and mutation helpers while retaining its uint facade. CausalSession
still implements the ordinary ulong links interface, whose wildcard is inside
the full stored u64 range. A checked wider adapter is required to separate it
from every real stored identity. The raw session contracts and passing
Rust/actual-WASM/JS protocol tests do not verify that remaining work.

- [x] First reproduce the existing monotonic allocation failure through
      AdvancedMixedQueryProcessor and EngineSession<uint>. Keep the smallest
      anonymous forward-reference case and independent explicit/pair-reuse
      controls; assert private and published rows separately.
- [ ] Add missing generic-query and checked-session API contracts before
      production changes. Cover all u64 identities, wildcard separation,
      lower caller reservations, quoted/encoded numeric and special names,
      owned observations and explicit-token publication with exact retries.
- [x] Select an unsigned generic-math address in the existing partial query
      implementation, retaining its public uint Options and Pattern facade,
      implicit string conversion and existing query semantics. Reuse the
      matching/mutation algorithms rather than copying them for causal use.
- [ ] Preserve reference kinds in QueryParser for the selected numeric type;
      reject malformed, negative and overflowing numeric input before checked
      private writes. Retain the established legacy uint interpretation.
- [x] Add optional query preparation and direct explicit-address capabilities
      to the ordinary named-store interface with compatible defaults. A
      monotonic session must not use the current create/delete prediction or
      walk every intervening address through LinksExtensions.EnsureCreated.
- [x] Under the allocator owner lock, execute the same query against owned
      preview rows/names with validation disabled, record its exact allocation
      order and validate final references. Reserve that
      sequence without creating probe rows; consume it in real creation and
      retire only a matching explicit reservation. Input and preview admission
      failures preserve rows and the allocator cursor. A later missing-reference
      rejection or rollback may retain already reserved gaps, matching Rust.
- [ ] Extend checked preview admission to decoded named reply budgets before
      real edits; the ordinary reservation capability alone does not bound replies.
- [ ] Bound input bytes, parser depth, document nodes, result/binding expansion
      and decoded named reply bytes before checked writes. Use finite bounded
      probes, ordinary positive controls and no host-exhaustion experiments.
- [ ] Extend standard protocol operations and server dispatch with the same
      selected address type, preserving all uint entry points. Carry original
      query text in the matching explicit envelope through text and binary;
      apply checked admission before recursive canonical formatting.
- [ ] Verify ordinary and causal allocation/concurrency/freeze/disposal cases,
      existing uint query/protocol suites and complete Debug/Release tests.
      Measure complete production coverage without removing denominators;
      run the existing source, changeset and release checks before committing.
- [ ] Add matching shared vectors and physical plain/mirrored query recipes,
      verify cross-language whole-store interchange and finite child cleanup,
      document costs and preserve before/after receipts. Commit useful steps,
      push only issue-138-adc2aa56c4cb and inspect exact-head platform CI.

Current source audit: AdvancedMixedQueryProcessor{,.Matching,.Mutations,
.References}.cs, QueryParser.cs, INamedTypes.cs, LinksExtensions.cs,
Engine/EngineSession.cs, Engine/CausalSession.cs and Protocol/LinksServer.cs.
The standard extension retains its allocator fallback for older stores and
explicit custom creator delegates. EngineSession now supplies direct ensure
and optional query reservations under its owner gate. Older named stores use
compatible default capability methods; they must opt in if their allocation
does not support the existing recycling prediction.

`bash experiments/issue-138/run-csharp-query-allocation.sh` reproduces the
anonymous point as `(2: 1 1)`, with an unpublished engine snapshot and missing
identity 1. The independent explicit point and explicit-high/pair-reuse
controls pass. `csharp-query-allocation-before.log` preserves the three finite
results before implementation. Each case creates at most four rows.

All other unmet R1–R12 criteria remain active. PR139 stays draft and unmerged.

Verified selected-address step: the same partial processor now accepts ushort, uint,
ulong and UInt128. The old uint Options and Pattern facade and implicit query
strings remain available. Existing rows at each selected maximum minus one,
and at ulong.MaxValue under UInt128, pass the finite focused contracts.
QueryParser classifies numbers with the selected type; checked malformed or
out-of-range admission remains unfinished.

The first generic build fails before implementation because the generic API is
missing. The first wide-row execution then exposes the pinned upstream
Link<UInt128> static initializer: NumericType uses field reflection for MinValue
and MaxValue, but UInt128 supplies properties. Store-selected constants already
use generic-math bounds. An owned internal three-part list now carries standard
callbacks and query rows without invoking those upstream list helpers. It costs
a small boxed read-only row per callback; it preserves observations across later
edits. The upstream consumer-side helper limitation is documented rather than
altering global constants or dependencies.

The independent dangling-zero swap fails before the substitution correction;
equality and join controls already pass. Removing the zero-to-wildcard rewrite
makes the eight focused contracts pass. Both complete Debug/Release suites pass
2,514 tests with zero failures or skips. All six causal components retain 100%
source lines and raw compiled branches. Complete production totals remain
10,220/10,531 lines and 6,629/7,080 branches in Debug; 7,900/8,103 lines and
6,569/7,037 branches in Release. Every previous production source file remains
included. Strict formatting, 221 repository tests and source/changeset/release/
ShellCheck gates pass. csharp-generic-query.txt preserves commands, before/after
failures, costs and source/report hashes. The allocation step below addresses
the separate anonymous failure; checked causal and physical query parity remain
unfinished.

Verified monotonic-allocation step: all ten permanent reproductions fail before
the fix; the separate three-case probe has two positive controls and one dangling
anonymous point. The corrected owned preview passes those ten cases plus eleven
ownership, concurrency, failure, wide-address, compatibility and preview contracts.
The first full draft exposes an existing blind-write validation regression:
reading every fresh address changes its conflict classification. Restricting that
collision read to reserved addresses restores the existing read-set contract.
Both complete Debug/Release suites pass 2,535 tests, zero failures or skips.
All source lines and raw compiled branches in the three new reservation sources
and the six existing causal components are visited. Complete production coverage
is 10,348/10,650 lines and 6,739/7,186 branches in Debug; 7,998/8,196 lines and
6,675/7,139 branches in Release. Every prior production file stays included.
The repository layout check also catches raw CI logs saved at the root; moving
them into experiments/issue-138/ci-logs preserves them and satisfies that gate.
csharp-query-reservations.txt retains reproduction commands, complete reports,
source hashes, successful full-suite logs and the initial draft failure.

The guarantee costs a detached visible-row/name snapshot, O(N log N) sorted
construction, a second query execution and O(K) reservation state. Queue
consumption and exact explicit retirement have expected O(1) cost. Raw sessions
allocate no reservation queue until needed. Checked causal admission, shared
query/protocol vectors, physical interchange and every wider unmet criterion
remain active; passing this step does not complete issue138.
