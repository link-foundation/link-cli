# Same-handle journal tail recovery

Requirement mapping: R1/R4/R7/R8. Constructor repair alone does not handle an
append that fails after writing a prefix while its writer remains alive.

- [x] Reproduce read, append and flush after partial failure in all three file
      adapters, with ASCII and incomplete UTF-8 tails. Use mocks for JS and
      actual Linux file-size quota failures in finite native child processes.
- [x] Track uncertain framing after I/O errors. Repair before further read,
      append or flush; preserve complete lines and reject complete corruption.
      Avoid rescanning successful appends or allocating with tail length.
- [x] Verify retry after repair failure, full-record/flush errors, zero-progress
      and short writes, exact reopening, writer fencing and unchanged raw paths.
- [x] Run focused/full suites, coverage and format/lint/type/release checks.
- [x] Preserve before logs, measured costs and limitations; update docs and release notes.
- [x] Commit and push only the prepared branch.
- [x] Verify fresh exact-head CI, all feedback and final diff.

Native children write one committed line and at most one 20,001-byte candidate.
A shell ignores SIGXFSZ; each native child applies an 8 KiB soft per-file limit
so the real syscall returns EFBIG. Each parent kills/joins a child after a finite deadline. Tests
requiring this Linux facility do not claim Windows/macOS quota coverage. Portable
file framing and existing reopening checks still run on those platforms.

The Linux write(2) reference documents partial writes at RLIMIT_FSIZE and later
EFBIG: https://www.man7.org/linux/man-pages/man2/write.2.html . Rust write_all
continues until completion or a non-interrupted error; an error can follow
successful earlier writes: https://doc.rust-lang.org/std/io/trait.Write.html .
The proposed response is an inference from those contracts and the existing
newline framing protocol, not a claim of device-sector atomicity.

The first C# child could not start CoreCLR: the quota also limited its JIT
`memfd` double mapping. This is a probe setup failure, not bug evidence. The
child alone now disables `DOTNET_EnableWriteXorExecute`; the parent and product
retain their defaults. Primary runtime explanation:
https://github.com/dotnet/runtime/discussions/81752 .

All six schedules per file adapter now fail behaviorally after a confirmed
partial write. C# reports Linux EFBIG as ArgumentOutOfRangeException on this
runtime; accepting that documented syscall result in the probe is separate
from verifying the later corrupted read/failed append. Initial probe startup
and error-type assumptions are retained separately, not counted as regressions.

The first coverage run completed the child journal checks, then failed while
Coverlet wrote its larger hit file under that same quota. The C# probe now sets
only the soft limit and restores it to the inherited hard limit after all
journal operations/disposal, including failure paths. This permits ordinary
managed exit and coverage collection without weakening the bounded I/O probe.
The limit is still checked as 8,192 bytes before journal access. Primary API
contract: https://www.man7.org/linux/man-pages/man2/getrlimit.2.html .

The initial full Rust coverage run also exposed a fixture interaction: inherited
stderr pointed to the parent log, which already exceeded 8 KiB. A quota-limited
child could not print its expected EFBIG diagnostic and panicked; a finite
9000-byte-stderr control reproduces exit 101. Rust now pipes child stderr and
reports it from the unrestricted parent only on failure. LLVM also needs more
than 8 KiB for its exit profile. A child-only quota guard applies the soft limit
after runtime startup and restores the original value after all journal access,
including assertion unwinding. Complete focused coverage must pass without
truncated profiles before the full suite is retried. These fixture failures are
separate from the six confirmed pre-fix product failures.

All nine workflows pass on published head 7826d95. See
`dev/log/issues/138/pulls/139/live-tail-ci.txt`. New code needs fresh CI.
