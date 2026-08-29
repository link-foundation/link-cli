---
'Foundation.Data.Doublets.Cli': minor
---

Opened the library up for extension: every decorator (`NamedTypesDecorator`, `NamedLinksDecorator`, `SimpleLinksDecorator`, `PinnedTypesDecorator`, `TransactionsDecorator`, `VersionControlDecorator`, `PersistentTransformationDecorator`) is now unsealed with overridable members, disposable ones follow the `protected virtual void Dispose(bool)` pattern so a subclass can release resources of its own, and `PersistentTransformationDecorator.PersistentTransformationQuery` and `InternalNamePrefix` are public. A custom CLI can now subclass any layer of the stack instead of forking it.
