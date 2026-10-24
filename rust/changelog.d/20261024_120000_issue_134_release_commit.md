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
