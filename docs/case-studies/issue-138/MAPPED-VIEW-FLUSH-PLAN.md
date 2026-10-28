# Legacy mapped-view flush ownership

Objective: supply an explicit native mapped-view/file durability barrier for
Rust's owned legacy mapping, preserve it through generic decorators, and expose
a lifetime-checked handle for caller-owned mappings. This advances R1/R6/R7/R8;
it does not supply physical torn-page repair or hardware power-loss certification.

- [x] Trace `DoubletsStorage::flush`, `PersistentFileMapped`, upstream
      platform-mem 0.3.0 and memmap2's Windows implementation. The existing
      adapter calls only `File::sync_all`; upstream Drop does the same.
- [x] Check primary Microsoft documentation: local mapped views are coherent,
      but durable mapped writes require FlushViewOfFile followed by
      FlushFileBuffers. C# already owns that ordered barrier.
- [x] Add portable native API/lifetime/adoption regressions before implementation
      and preserve their original missing-capability failure.
- [x] Retain a mapped view descriptor only while its owner is alive. Invalidate
      it before remapping and close; refuse stale/failed ownership. Flush the
      actual mapped view before its owned file handle; propagate both errors.
- [x] Preserve automatic ownership through map_store and provide explicit
      attachment for adopted generic stores, without changing their raw API.
- [x] Verify ordered/failing barriers with independent syscall mocks and real
      file growth/shrink/reopening controls. Bound inputs and reap any children.
- [ ] Run relevant complete suites and local checks sequentially, publish a
      useful atomic step and inspect fresh Windows/Linux/macOS exact-head CI.

Local validation finishes 2026-10-27T09:13:25Z: all twelve new native controls,
six default-feature controls, the adoption example, 1,175 complete native tests,
six doctests, 723 actual-WASM tests and 221 repository checks pass. Full LLVM
denominators remain included; native and whole-linked WASM remain below 100%.
The independent allocation assertion first caught a tail-only range in the
draft, then passed with the complete current mapping. See mapped-flush.txt and
mapped-flush-before.txt. Publication and exact-head platform CI remain pending.

Primary references checked 2026-10-27:

- https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-flushviewoffile
- https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-mapviewoffile
- https://docs.rs/platform-mem/0.3.0/platform_mem/struct.FileMapped.html

The Windows API ordering is a source-established requirement. Ordinary readback
and killed-process tests cannot reproduce hardware power loss. Generic foreign
stores do not expose their mapped views; their adoption requires an explicit
matching flush owner. Existing upstream failed-resize safety and legacy physical
repair remain separate concerns and must not be inferred fixed by this barrier.

Exact published 89fbfad passes all Rust platforms and actual Windows owned-view
controls. Eight workflows pass; C# Linux fails its finite-progress supervisor
with phase 5 / exit 0. `mapped-flush-ci-after.txt` records timestamps, run links
and log lines. The separate PHASE-POOL-PLAN.md tracks that proven helper
dependency. This head is not described as globally green.
