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

The NuGet and GitHub release lookups that decide whether to release now retry
network failures, 429 and 5xx with backoff and report the network cause, so a
single dropped connection no longer fails the release.

A changeset whose bump type cannot be parsed now fails the release with an
"Invalid changeset" error naming the file. It used to be skipped with a
warning and then deleted, so its bump and description were silently lost.

`LinksServer` no longer crashes the process when it shuts down right after
accepting a connection. The worker used to get its stream while `Shutdown`
disposed the client, and `TcpClient` throws `ArgumentNullException` when that
happens midway; the stream is now taken in the accept loop, under the lock
`Shutdown` holds.
