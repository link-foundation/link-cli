// @link-foundation/link-cli-server: the Rust `clink` in JavaScript, as an
// embedded store and as a server. Open a store with `NamedTypesDecorator.open`,
// run queries over it with `QueryProcessor`, wrap it in transactions, version
// control, triggers or permissions, serve it with `LinksServer` or
// `GraphqlServer`, or run the whole command line with `main`.

export { main, runQueryPipelineWith, openNamedStorage } from './main.js';
export { Cli } from './cli.js';
export { formatErrorReport, reportError } from './error-report.js';
export {
  AddressOverflowError,
  InvalidFormatError,
  LinkError,
  LockError,
  NotFoundError,
  ParseError,
  QueryError,
  StorageError,
  TransactionError,
} from './errors.js';

// The store: links, names, types, Unicode strings, LiNo and store archives.
export { ANY, MAX_ADDRESS, NULL_LINK, link } from './link.js';
export { LinkStorage } from './link-storage.js';
export { NamedTypeLinks } from './named-type-links.js';
export { NamedTypesDecorator, makeNamesDatabaseFilename } from './named-types.js';
export { TypesDecorator, TypeMarkers } from './types.js';
export { UnicodeStringStorage } from './unicode-string-storage.js';
export { PinnedTypes } from './pinned-types.js';
export { IndexedLinks } from './indexed-links.js';
export { importLinoFile, importLinoText } from './lino-database-input.js';
export {
  exportStore,
  exportStoreFile,
  importStore,
  importStoreFile,
} from './protocol-store-file.js';
export { formatStructure } from './structure-format.js';
export { parseLino } from './parser.js';
export { parseLinoToLinks, LinoParseError } from './protocol-lino-parser.js';

// The query language.
export { QueryProcessor } from './query-processor.js';
export { QueryLimits, QueryOptions } from './query-options.js';
export { LinkReferenceValidator } from './link-reference-validator.js';

// Store policies: undo, transactions, version control, triggers, permissions.
export { UndoableLinksDecorator } from './undoable-links.js';
export { TransactionsDecorator } from './transactions.js';
export { VersionControlDecorator } from './version-control.js';
export {
  PersistentTransformationDecorator,
  PersistentTransformationKind,
  TriggerStore,
} from './persistent-transformations.js';
export {
  AccessPolicy,
  Forbidden,
  LinksTokenTable,
  PermissionAccess,
  PermissionPolicy,
  PermissionsDecorator,
} from './permissions.js';

// Triplets over doublets.
export { TripletsOverDoublets, TripletMarkers } from './triplets.js';
export { TripletsPermissionsDecorator } from './triplets-permissions.js';

// Serving: the clink protocol over TCP and every IPC endpoint, and GraphQL.
export { LinksServer, ServerOptions, AcceptedProtocols } from './protocol-server.js';
export { LinksSessions } from './protocol-sessions.js';
export { LinksClient } from './protocol-client.js';
export { Endpoint, connect, listen, listenAll } from './ipc.js';
export { GraphqlServer } from './graphql-server.js';
export { Settings as GraphqlSettings } from './graphql-execute.js';
export { DeepStore } from './graphql-store.js';
