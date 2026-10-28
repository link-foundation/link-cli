# Unresolved generic mapped growth failure

The new finite Linux probe reproduces an actual failure still affecting
R1/R6/R7/R8. It extends the existing real-quota experiment while retaining its
original plain-data controls. Production mapping code is unchanged.

```sh
CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 \
  RUSTFLAGS='-C link-arg=-Wl,--threads=1' \
  cargo build --manifest-path experiments/issue-138/mapped-grow-failure/Cargo.toml
python3 experiments/issue-138/run-mapped-grow-failure.py --expect-retained
python3 experiments/issue-138/run-mapped-grow-failure.py --generic-repro
```

The first command checks plain-data growth within and beyond a real 4096-byte
file quota: both children exit successfully, retaining the live prefix and
checked flush after rejection. The second checks a transparent integer type
with an empty destructor and no pointer or owned resource. All byte patterns
are valid, and `needs_drop` is explicitly asserted. Its successful growth
control passes. Beyond the quota, growth returns OS error 27 and the checked
flush refuses its invalidated descriptor. A subsequent safe public prefix read
crashes with SIGSEGV. The parent expects and labels this unresolved defect.

Both scripts bound child address space to 256 MiB, stack to 8 MiB, file size
to one page, CPU to 5/6 seconds and lifetime to 30 seconds. Core dumps are
disabled, output is bounded, SIGXFSZ is ignored to inspect the returned error,
and every child is reaped. The failing request adds only 8192 u64-sized items.
This experiment belongs outside the default test suite; expecting its crash
does not turn it into a verified fix.

The locked platform-mem 0.3.0 owner unmaps before fallible metadata, resize and
remapping calls. Its allocation descriptor remains unchanged on early error,
so both subsequent access and destruction can use an unmapped pointer. The
wrapper refuses its flush descriptor but still owns that upstream object.
The separate retained-growth path fixes plain data; applying overlapping owned
mappings to arbitrary destructor-bearing values would duplicate destruction.

The public compatibility surface exposes the exact upstream
`doublets::mem::FileMapped<T>` reference and unrestricted `RawMem<T>` methods.
The examined upstream API provides no public detach/reset operation. Changing
generic type identity or requiring `Clone` would remove accepted callers.
Guessing private field layout, leaking an owner or retrying a fallible remap
without handling another failure does not provide a sound general correction.

The newly published platform-mem 0.4.0 fixes initialization semantics, but its
[pinned owner source](https://github.com/linksplatform/mem-rs/blob/f3362cfe2abef9f99fd23c98776efe3bdadd5ff5/src/file_mapped.rs#L99)
still unmaps before the fallible operations and keeps the owner fields private.
This is source review, not a claim that the executable tested version 0.4.0;
the actual dependency remains 0.3.0 through doublets 0.9.0.

[The receipt](../../../dev/log/issues/138/pulls/139/mapped-generic-failure.txt)
retains commands, exact dependency/source identities, returned error, child
outcomes, public API audit and strict focused lint results. A compatible sound
fix remains unfinished; this audit is evidence of the blocker. Partial shrink,
callback/destructor panic, physical repair and every wider unmet criterion
remain open. PR139 remains draft and unmerged.
