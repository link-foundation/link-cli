# Retain the live Rust mapping during rejected growth

Objective: preserve the existing pointer, contents and flush ownership when
growth of ordinary doublets/plain-data mappings fails. Keep the public raw
mapping API and existing generic destructor behavior. This advances R1/R6/R7/
R8/R9/R11 without claiming shrink failure repair or physical page atomicity.

- [x] Reproduce actual rejected growth in a finite Linux quota child. A request
      of about 66 KiB under a 4 KiB file quota returns EFBIG, then the old safe
      allocation read terminates with SIGSEGV. A within-quota control passes.
      Disable cores, bound address space/stack/CPU/output and reap both children.
- [x] Trace platform-mem 0.3.0: grow drops its old mapping before metadata,
      extension and replacement mapping can fail. The raw buffer and outer
      doublets trees retain the previous address on an error.
- [x] Add durable automated quota, overflow, initialization and panic controls
      before production changes and retain their before failures.
- [x] Prepare a separate FileMapped owner before releasing a drop-free live
      mapping. Preserve original file-initialization bounds after failed file
      extension; fill only genuine new items. Keep generic values with Drop on
      their existing path, since dropping two overlapping owners is unsound.
- [x] Extend Windows files through a temporary mapping object while the old
      view remains alive; never call SetEndOfFile with that live view.
- [x] Independently inject preparation errors and verify pointer/flush retention,
      following edits, retry, reopening, exact initialization and resource cleanup.
- [x] Run full local suites sequentially and retain complete coverage denominators
      in mapped-grow.txt: 1,184 native, six doctests, 723 actual-WASM and all
      221 repository checks pass, finishing 2026-10-27T10:22:42Z.
- [ ] Publish an atomic correction and inspect fresh exact-head platform CI.

The earlier explicit flush barrier correctly rejects an invalid descriptor; it
does not make the old allocator's pointer readable. The replacement must keep
the real mapping, not merely change the error message. Existing arbitrary Drop
values and failed shrink remain separate limits. No private upstream layout is
accessed, and no dependency patch is silently lost on package publication.

Primary references checked 2026-10-27:

- https://docs.rs/crate/platform-mem/0.3.0/source/src/file_mapped.rs
- https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-createfilemappingw
- https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setendoffile

CreateFileMapping can extend the file while retaining the old local view.
SetEndOfFile requires first releasing every mapped view and mapping object.
Successful process/readback controls are not hardware power-loss certification.
