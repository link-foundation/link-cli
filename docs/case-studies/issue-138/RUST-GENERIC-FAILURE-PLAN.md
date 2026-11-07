# Generic mapped failure boundary

R1/R6/R7/R8 still require real failure safety for the public generic mapping
API. Earlier successful initialization and plain-data retained growth do not
establish that guarantee for destructor-bearing values. Keep the same public
`PersistentFileMapped<T>` and `inner() -> &doublets::mem::FileMapped<T>` surface.

- [x] Inspect the wrapper, dependency growth/shrink order and existing quota probe.
- [x] Extend the finite child probe with a valid integer representation that has
      drop glue; preserve plain-data controls and the existing runner defaults.
- [x] Force a genuine growth error with Linux RLIMIT_FSIZE; disable core dumps,
      bound address space/stack/CPU and output, and reap every child.
- [x] Record the returned error, flush admission and safe public prefix access.
- [x] Identify whether public upstream APIs can retain/recover the old owner
      without changing exposed types, leaking owners or duplicating destruction.
- [x] Implement and verify a sound compatible fix if the available APIs permit it.
- [x] Preserve exact source identities, commands, evidence and unresolved criteria.

A crashing isolated negative control establishes a library defect; it is not a
fix or an ordinary passing CI regression. Do not add uncontrolled crash tests
to the default suite, make resource limits larger, or replace a failed owner
through guessed private layouts. Full suites remain in CI.

The plain-data quota controls pass. Generic growth used to return OS error 27
and then crash with SIGSEGV on the next safe prefix read. It now returns the error
and keeps the view readable. See RUST-GENERIC-FAILURE-VERIFICATION.md for the fix
and for what still depends on upstream.
