---
'Foundation.Data.Doublets.Cli': major
---

Made the C# transactions layer reusable from outside the CLI, matching
what the Rust library now offers (issue #98).

`TransactionsDecorator` is generic over the doublets address type:
`TransactionsDecorator<TLinkAddress>` works over any
`INamedTypesLinks<TLinkAddress>` whose address is an
`IUnsignedNumber<TLinkAddress>`, so a consumer with a `ulong`-addressed
store is a first-class user rather than being pinned to `uint`. The
non-generic `TransactionsDecorator` remains as a `uint` specialisation,
so existing code that constructs it keeps compiling. The transitions
wire format is unchanged and address-type independent — addresses are
written in decimal under the invariant culture, so a log written by a
`uint`-addressed store reads back unchanged in a `ulong`-addressed one,
and an address that does not fit the target type is rejected instead of
being silently truncated.

New `LinksFileLock` and `StorageRevision` cover multi-process access:
advisory locking of a database's `.lock` sidecar (shared for readers,
exclusive for writers, with a blocking `Acquire` and a non-blocking
`TryAcquire`) and a cheap "has anyone else written since I last looked?"
fingerprint. The lock file path and the shared/exclusive semantics match
the Rust `storage::lock` module, so the two implementations can guard the
same database.

Source-breaking: `Transition`, `ITransaction` and `ITransactionsLinks`
are now generic. Existing `uint` code should use `Transition<uint>`,
`ITransaction<uint>` and `ITransactionsLinks<uint>`.
