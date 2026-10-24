---
bump: patch
---

Fix the release pipeline so a Rust release always commits its version bump.
`rust/CHANGELOG.md` did not exist, so the release script's `git add` failed
silently, nothing was committed and every push to main re-published the same
changelog fragments as a new patch version (1.0.6, 1.0.7 and 1.0.8). The
script now creates `CHANGELOG.md`, consumes the fragments, syncs `Cargo.lock`,
verifies the release write, fails on any staging error, retries only lost push
races and tags the release only after its commit reached the remote.

crates.io trusted publishing now runs only when the
`CRATES_IO_TRUSTED_PUBLISHING` repository variable is `true`. Attempting it
unconditionally left a "No Trusted Publishing config found" error annotation on
every successful release that published with the token secret.

The crates.io, Docker Hub and GitHub release lookups that decide whether and
which version to release now retry network failures, 429 and 5xx with backoff
and fail with an error once their attempts run out. A failed lookup used to
count as "not published", so a single dropped connection re-released a
published version or bumped from a stale base.
