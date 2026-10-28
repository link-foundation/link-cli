# C# standard query path over private sessions

Continue CAUSAL-PROTOCOL-PLAN.md. The C# processor now shares selected-address
matching and mutation helpers while retaining its uint facade. CausalSession
also exposes checked QueryLinks through UInt128, separating its wildcard from
every stored u64 identity. Standard C# protocol dispatch is implemented; physical
query recipes await complete interchange CI. The raw session contracts and passing
Rust/actual-WASM/JS protocol tests do not verify that remaining work.

- [x] First reproduce the existing monotonic allocation failure through
      AdvancedMixedQueryProcessor and EngineSession<uint>. Keep the smallest
      anonymous forward-reference case and independent explicit/pair-reuse
      controls; assert private and published rows separately.
- [x] Add missing generic-query and checked-session API contracts before
      production changes. Cover all u64 identities, wildcard separation,
      lower caller reservations, quoted/encoded numeric and special names,
      owned observations and explicit-token publication with exact retries.
- [x] Select an unsigned generic-math address in the existing partial query
      implementation, retaining its public uint Options and Pattern facade,
      implicit string conversion and existing query semantics. Reuse the
      matching/mutation algorithms rather than copying them for causal use.
- [x] Preserve reference kinds in QueryParser for the selected numeric type;
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
- [x] Extend checked preview admission to decoded named reply budgets before
      real edits; the ordinary reservation capability alone does not bound replies.
- [x] Bound input bytes, parser depth, document nodes, result/binding expansion
      and decoded named reply bytes before checked writes. Use finite bounded
      probes, ordinary positive controls and no host-exhaustion experiments.
- [x] Extend standard protocol operations and server dispatch with the same
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
QueryParser classifies numbers with the selected type. The checked adapter now
rejects negative and out-of-u64 numeric tokens before private edits, while quoted
and encoded references keep their name semantics.

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
allocate no reservation queue until needed. The checked causal API now consumes
the shared query vectors; standard protocol vectors, physical interchange and
every wider unmet criterion remain active. Passing this step does not complete
issue138.


Checked C# causal queries finish complete local validation at
2026-10-28T00:25:35Z. Missing QueryLinks APIs first fail in the standalone probe
and permanent tests. The shared ordinary processor now runs against an owned
UInt128 adapter over the existing private session, preserving every stored u64
identity separately from its wildcard. All sixteen independent shared vectors
pass, including quoted numeric/special names, zero bindings and lower fresh
reservations. Thirty-six new cases additionally verify detached observations,
frozen/disposed adapters, exact token retries after unknown publication,
negative/overflow/range refusal, original UTF-8 bytes, depth/nodes, result and
binding expansion, and decoded named reply refusal before real edits/callbacks.
A retained deleted maximum can be resurrected without advancing a lower fresh
cursor. Standard uint query behavior remains covered by the complete suites.

Both full Debug and Release configurations pass 2571 tests with zero failures
or skips. All six existing causal sources retain every source line and raw
compiled branch. Complete production coverage includes all 109 preceding files
and six additional files: Debug 10528/10828 lines and 6900/7360 branches;
Release 8143/8341 and 6833/7309. New checked-query source branch gaps and the
preview's uncovered range-refusal line remain listed in csharp-causal-query.txt;
this is not whole-project 100%. Strict formatting, 221 repository tests and
source-size/changeset/release-helper/ShellCheck/whitespace gates pass.

The outer wrapper returns 127 after these successful subprocesses because its
log-output name was edited while Bash was reading it. The stable corrected
wrapper passes syntax and ShellCheck; the sixteen-vector probe is rerun into a
distinct log. The receipt preserves that error and does not claim a second full
suite for the naming-only correction. Rust/JS production bytes still match the
preceding complete protocol validation. The prior generic C# head's Windows
Rust job was cancelled after the allocation head started; the cancellation and
raw log line 41256 remain in csharp-generic-query-head-ci.txt.

Costs remain an O(N log N) detached sorted snapshot, repeated ordinary query
execution, O(K) lazy FIFO reservations and finite matching/binding/change maps.
Direct checked Count/Each own matched rows and may refuse broad reads. Standard
selected-address C# protocols, the lossless query envelope, physical query
interchange, complete native/C#/linked-WASM 100%, fresh exact-head CI and every
wider unmet R1-R12 criterion remain unfinished. PR139 stays draft and unmerged.
