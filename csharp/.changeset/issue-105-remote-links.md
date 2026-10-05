---
'Foundation.Data.Doublets.Cli': minor
---

Added `RemoteLinks`, an `INamedTypesLinks<uint>` whose links live behind a `LinksServer`. Code written against the links interface, the query processor included, switches from a local file to a server by swapping one value. `LinksServer` now also answers `LinksOperation` documents, one per interface call: `(count: …)`, `(each: …)`, `(create: …)`, `(update: …)`, `(delete: …)`, `(get-name: …)`, `(set-name: …)`, `(get-by-name: …)` and `(remove-name: …)`. These are the documents the Rust `RemoteLinks` sends, so either port's client works with either port's server. Both test suites replay the same recorded conversation, `docs/protocol/links-operations.txt`, to prove it. A write replies with the net change of every link it touched.

Fixed a crash of a `LinksServer` shut down right after it accepted a connection: the connection thread set an option on the socket the shutdown had already disposed, and the `NullReferenceException` ended the process.
