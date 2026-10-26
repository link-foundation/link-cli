# Native immutable doublets pages

Requirement mapping: R1/R5/R6/R7/R8/R9/R11 and the maintainer's native-storage
request. This is an implementation plan, not evidence of a completed provider.
The pinned LMDB source study in RESEARCH.md motivates alternating durable roots
and immutable committed pages. The proposed format is our own doublets format;
neither LMDB compatibility nor equivalent implementation maturity is claimed.

- [x] Specify an independent binary wire vector before implementing codecs.
- [x] Add failing public ordered-log contracts before provider implementation.
- [ ] Implement immutable, checksummed native doublets pages and alternating
      metadata decisions behind the existing commit-log API, without SQL.
- [ ] Preserve matching Rust, C# and JS implementations, including actual WASM
      engine use through its host log and Node/Bun/Deno cross-language reopening.
- [ ] Exercise finite durable-image write/sync failures, torn decisions/pages,
      corrupt committed data, stale database pages and uncertain owner fencing.
- [ ] Exercise bounded real child exits and subsequent writes in each native
      provider. Never automatically steal a crashed writer's exclusive-create lock.
- [ ] Verify selected costs with finite page/byte counters, preserving existing
      default stores and independent logical encoding/replication strategies.
- [ ] Add independently selected mirrored page repair with matching finite
      corruption tests; distinguish its storage cost from a doublewrite buffer.
- [ ] Run complete suites, coverage, runnable examples and fresh exact-head CI;
      update the native capability comparison with precise persistence limits.

The candidate stores ordered UTF-8 records as doublets in append-only 4,096-byte
pages. Each page has a 64-byte header and 252 source/target pairs of 64-bit values.
Row addresses derive from page and slot; unused committed slots are never reused.
String chunks use tagged external numeric values and linked doublets; record
doublets refer to the string root and preceding record. Pages bind a database ID,
generation, physical page index and checksum chain. Two fixed-size metadata files
alternate the published generation, counts and last root/checksum. Data reaches
storage before a metadata decision, which reaches storage before acknowledgement.

Recovery selects the newest valid decision, validates its entire committed page
prefix and rejects committed corruption rather than falling back silently to an
older acknowledged state. Unpublished data tails can be truncated only by the
exclusive owner. Failed write/sync outcomes fence that handle until reopening.
Every implementation must consume independent vectors and the other providers'
files; matching copies of one codec's output alone are insufficient evidence.

Native adapters must share the same atomic exclusive-create ownership convention to
prevent simultaneous writers across languages. Crashed-owner recovery requires
external fencing before explicit lock removal. Directory/file synchronization,
device assumptions and Windows differences must be documented and tested.
Finite durable-image tests model persistence failures; they cannot establish
behavior of every filesystem/controller under physical power loss.

This provides one selected native shadow-page strategy. Legacy in-place mapped
page repair, efficient page undo, LSM, persistent object retention, balancing,
multi-catalog transactions and whole-project coverage remain separate unfinished
criteria. The plan must not be used to mark any of those complete.

Prototype evidence: NATIVE-PAGES-FORMAT.md and native-pages-prototype.txt.
Eleven finite JS investigation tests and actual-WASM Node/Bun/Deno reopening
pass. Production integration and Rust/C# parity are still unchecked above.

JS/TS production integration now passes 197 full JS tests, enforced 100%
production coverage, strict types, 221 repository checks and actual-WASM
Node/Bun/Deno conformance under 16 independent policy configurations. Native
files share the same ownership convention. Finite before evidence preserves
an uncertain-close fence regression and its passing correction. See
native-pages-js.txt and ENGINE-NATIVE-PAGES.md. Rust/C# parity, cross-language
files and selected redundant repair remain required before the main boxes can
be checked. The existing 0.2.0 client release preparation covers the new exports.
