---
'Foundation.Data.Doublets.Cli': minor
---

Add an optional permission layer with permissions stored in links (#149), the port of `createPolicy` and `withPermissions` of `@link-foundation/ui-links` with its tests. A rule is a pair `(g: Subject Object)` marked by `(r: AllowRead g)` or `(r: AllowWrite g)`; `Anything` matches everything and write implies read. `PermissionPolicy<TLinkAddress>` reads the rules from the data or from a store of their own, and `PermissionsDecorator<TLinkAddress>` is the store as a subject sees it: reads skip what it may not read, and a create, update or delete it may not make throws `ForbiddenException` (`Forbidden: update 14`) and changes nothing. A link the subject created is its own while its source is 0. `UndoableLinksDecorator<TLinkAddress>` makes a request atomic, and the query processor checks references in the store below the view (`Options.References`).

`clink` gains `--permissions self|<rules.links>`, `--subject`, `--tokens`, `--grant-token`, `--revoke-token` and `--token`. A server with `LinksServerOptions.Permissions` gives every connection a session (`LinksSessions`); `(auth: token)` replies `(subject: i)` through `LinksServerOptions.Authenticate`, any callback, or `LinksTokenTable`, a token table stored as links named after the SHA-256 of each token. Without `--permissions` nothing changes.

The dependency `Platform.Data.Doublets` is updated to 0.18.7.
