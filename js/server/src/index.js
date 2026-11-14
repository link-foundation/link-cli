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
export { FileLock, LockMode, lockFilePath } from './storage-lock.js';
export { NamedTypeLinks } from './named-type-links.js';
export { NamedTypesDecorator, makeNamesDatabaseFilename } from './named-types.js';
export { EXTENDS_NAME, ROOT_TYPE_NAME, TypeMarkers, TypesDecorator, ensurePoint } from './types.js';
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
export { LinksUndoLog, UndoableLinksDecorator } from './undoable-links.js';
export { TransactionsDecorator } from './transactions.js';
export { FileTransitionLog } from './transactions-log.js';
export {
  CommitMode,
  INFINITE_RETENTION,
  RowUndoPolicy,
  TransitionKind,
  parseRetentionPolicy,
} from './transactions-types.js';
export {
  DEFAULT_BRANCH_NAME,
  VersionControlDecorator,
  makeVersionControlDatabaseFilename,
} from './version-control.js';
export {
  INTERNAL_NAME_PREFIX,
  PersistentTransformation,
  PersistentTransformationDecorator,
  PersistentTransformationKind,
  PersistentTransformationQuery,
  TriggerStore,
  makeTriggersDatabaseFilename,
} from './persistent-transformations.js';
export {
  AccessPolicy,
  Forbidden,
  LinksTokenTable,
  PermissionAccess,
  PermissionMarkers,
  PermissionPolicy,
  PermissionsDecorator,
} from './permissions.js';

// Triplets over doublets.
export {
  DoubletRole,
  NOTHING_MARKER_NAME,
  PAIR_MARKER_NAME,
  TripletMarkers,
  TripletsOverDoublets,
} from './triplets.js';
export { TripletQuery } from './triplets-restriction.js';
export { processTripletsQuery } from './triplets-query.js';
export { importTripletsLino, tripletsToLino } from './triplets-lino.js';
export { TripletsPermissionsDecorator } from './triplets-permissions.js';

// Serving: the clink protocol over TCP and every IPC endpoint, and GraphQL.
export { LinksServer, ServerOptions, AcceptedProtocols } from './protocol-server.js';
export { LinksSessions } from './protocol-sessions.js';
export { LinksClient } from './protocol-client.js';
export { BinaryLinoProtocol, ProtocolLimits, TextLinoProtocol } from './protocol-protocols.js';
export { BinaryLinoOptions } from './protocol-binary.js';
export { Endpoint, connect, listen, listenAll } from './ipc.js';
export { GraphqlServer } from './graphql-server.js';
export { Settings as GraphqlSettings } from './graphql-execute.js';
export { DeepStore } from './graphql-store.js';
