---
'Foundation.Data.Doublets.Cli': patch
---

Harden the C# release automation (issue #134): the release commit is rebased
onto the latest main before any file is touched, a push that loses the race to
a concurrent release is rebased and retried while a repository-rule rejection
fails fast with an explicit error, the tag is created only after the commit
reached main and exactly that tag is pushed, git runs without a shell so
release descriptions are passed verbatim, and CHANGELOG.md no longer gains an
extra blank line on every release.
