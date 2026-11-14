#!/usr/bin/env node
// Generates the first parity/features.json (issue #111). Kept so the initial
// classification can be audited; afterwards the manifest is edited by hand and
// refreshed with `node parity/check-parity.mjs --update`.
import { writeFileSync } from 'node:fs';

const C = 'csharp/Foundation.Data.Doublets.Cli.Library/';
const CLI = 'csharp/Foundation.Data.Doublets.Cli/';
const CT = 'csharp/Foundation.Data.Doublets.Cli.Tests/';
const R = 'rust/src/';
const RT = 'rust/tests/';
const J = 'js/client/src/';
const JT = 'js/client/test/';
const p = (prefix, names) => names.map((name) => prefix + name);
const impl = (sources, tests) => ({ status: 'implemented', sources, tests });
const viaWasm = {
  status: 'delegated',
  to: 'rust',
  via: ['rust/wasm/src/lib.rs', 'js/client/src/wasm.js', 'js/src/App.jsx'],
  tests: ['rust/wasm/tests/core.rs', 'js/client/test/wasm.test.mjs'],
};
const na = (reason) => ({ status: 'not-applicable', reason });
const serverOnly = na(
  'Server-side store policy. JavaScript reaches it as a protocol client (js/client/src/client.js); the WASM build does not expose it.',
);

const features = [
  {
    id: 'query-language',
    title: 'LiNo query language: parse, match, create, update, delete, substitute',
    requirements: 'docs/REQUIREMENTS.md#query-language',
    csharp: impl(
      p(C, [
        'QueryParser.cs',
        'QueryConstants.cs',
        'QueryLimits.cs',
        'QueryPreviewLease.cs',
        'BasicQueryProcessor.cs',
        'MixedQueryProcessor.cs',
        'AdvancedMixedQueryProcessor*.cs',
        'LinkParts.cs',
        'EnumerableExtensions.cs',
      ]),
      p(CT, [
        'BasicQueryProcessor.cs',
        'MixedQueryProcessor.cs',
        'AdvancedMixedQueryProcessor*.cs',
        'Query*.cs',
        'GenericQueryAddressTests.cs',
      ]),
    ),
    rust: impl(
      p(R, [
        'parser.rs',
        'query_parser.rs',
        'query_processor.rs',
        'query_processor/**',
        'query_processor_substitution.rs',
        'query_types.rs',
        'query_options.rs',
        'query_address.rs',
        'query_reply_budget.rs',
        'link_reference_validator.rs',
        'hybrid_reference.rs',
        'lino_link.rs',
        'link.rs',
      ]),
      p(RT, [
        'parser_tests.rs',
        'query_*_tests.rs',
        'quoted_query_references_tests.rs',
        'lino_link_tests.rs',
        'link_tests.rs',
      ]),
    ),
    javascript: viaWasm,
  },
  {
    id: 'change-reports',
    title: 'Simplified before/after change reports',
    csharp: impl(p(C, ['ChangesSimplifier.cs']), p(CT, ['ChangesSimplifier.cs'])),
    rust: impl(p(R, ['changes_simplifier.rs']), p(RT, ['changes_simplifier_tests.rs'])),
    javascript: viaWasm,
  },
  {
    id: 'names',
    title: 'Named links and named types',
    csharp: impl(
      p(C, [
        'NamedLinks.cs',
        'NamedLinksDecorator.cs',
        'NamedTypesDecorator*.cs',
        'NamesDatabase.cs',
        'INamedTypes.cs',
      ]),
      p(CT, ['NamedLinksDecoratorTests.cs', 'NamedTypesDecoratorTests.cs']),
    ),
    rust: impl(
      p(R, ['named_links.rs', 'named_type_links.rs', 'named_types.rs']),
      p(RT, [
        'named_types_decorator_tests.rs',
        'named_type_links_bridge_tests.rs',
        'inline_name_persistence_tests.rs',
      ]),
    ),
    javascript: viaWasm,
  },
  {
    id: 'pinned-types',
    title: 'Pinned type links',
    csharp: impl(p(C, ['PinnedTypes*.cs']), p(CT, ['PinnedTypes*.cs'])),
    rust: impl(p(R, ['pinned_types.rs']), p(RT, ['pinned_types_decorator_tests.rs'])),
    javascript: viaWasm,
  },
  {
    id: 'types',
    title: 'Type links decorator',
    csharp: impl(p(C, ['TypesDecorator.cs']), p(CT, ['TypesDecoratorTests.cs'])),
    rust: impl(p(R, ['types.rs']), p(RT, ['types_decorator_tests.rs'])),
    javascript: viaWasm,
  },
  {
    id: 'unicode-strings',
    title: 'Unicode strings stored as link sequences',
    csharp: impl(
      p(C, ['UnicodeStringStorage.cs', 'CheckedUnicodeSequenceWalker.cs']),
      p(CT, ['UnicodeStringStorageTests.cs', 'UnicodeGraph*.cs']),
    ),
    rust: impl(p(R, ['unicode_string_storage.rs', 'sequences/**']), p(RT, ['unicode_*_tests.rs'])),
    javascript: viaWasm,
  },
  {
    id: 'lino-input-output',
    title: 'LiNo import, export and --structure output',
    csharp: impl(
      p(C, ['LinoDatabaseInput.cs', 'LinoDatabaseOutput.cs']),
      p(CT, [
        'LinoDatabase*Tests.cs',
        'Structure*Tests.cs',
        'CliExportIntegrationTests.cs',
        'CliImportAtomicityTests.cs',
      ]),
    ),
    rust: impl(
      p(R, ['lino_database_input.rs', 'text_storage_format.rs', 'structure_format.rs']),
      p(RT, [
        'lino_database_input_tests.rs',
        'structure_*_tests.rs',
        'cli_export_tests.rs',
        'cli_import_tests.rs',
      ]),
    ),
    javascript: viaWasm,
  },
  {
    id: 'persistent-transformations',
    title: 'Persistent transformations (triggers)',
    csharp: impl(
      p(C, ['PersistentTransformationDecorator.cs']),
      p(CT, ['PersistentTransformationDecoratorTests.cs']),
    ),
    rust: impl(
      p(R, ['persistent_transformations.rs']),
      p(RT, ['persistent_transformations*_tests.rs', 'cli_triggers_tests.rs']),
    ),
    javascript: serverOnly,
  },
  {
    id: 'transactions',
    title: 'Transactions, undo images, journal and crash recovery',
    csharp: impl(
      p(C, [
        'TransactionsDecorator*.cs',
        'TransactionUndoImage.cs',
        'JournalRecord.cs',
        'LinksUndoLog.cs',
        'IDurableLinks.cs',
        'DurableMappedMemory.cs',
        'LinksFileLock.cs',
        'ChunkArchive.cs',
      ]),
      p(CT, [
        'Transaction*.cs',
        'RecoveryValidationTests.cs',
        'LinksFileLockTests.cs',
        'ChunkArchiveOwnershipTests.cs',
      ]),
    ),
    rust: impl(
      p(R, ['transactions/**', 'undoable_links.rs']),
      p(RT, ['transactions_*_tests.rs', 'transaction_*_tests.rs', 'undoable_links_edges_tests.rs']),
    ),
    javascript: serverOnly,
  },
  {
    id: 'version-control',
    title: 'Versions: commits, ancestry, movement and publication',
    csharp: impl(p(C, ['VersionControlDecorator*.cs']), p(CT, ['Version*.cs'])),
    rust: impl(p(R, ['version_control/**']), p(RT, ['version_*_tests.rs'])),
    javascript: serverOnly,
  },
  {
    id: 'triplets',
    title: 'Triplets over doublets and the triplets protocol',
    csharp: impl(
      p(C, ['Triplets*.cs', 'Protocol/RemoteTriplets.cs', 'Protocol/TripletsOperation.cs']),
      p(CT, [
        'TripletsOverDoubletsTests.cs',
        'TripletsPermissionsDirectTests.cs',
        'Protocol/TripletsProtocolTests.cs',
      ]),
    ),
    rust: impl(
      p(R, ['triplets/**', 'protocol/remote_triplets.rs', 'protocol/triplets_operations.rs']),
      p(RT, ['triplets_*_tests.rs']),
    ),
    javascript: serverOnly,
  },
  {
    id: 'permissions',
    title: 'Permissions, tokens and authenticated sessions',
    csharp: impl(
      p(C, ['Permissions.cs', 'PermissionsDecorator.cs', 'LinksTokenTable.cs']).concat([
        `${CLI}CliPermissions.cs`,
      ]),
      p(CT, ['Permissions*.cs', 'CliPermissionsTests.cs']),
    ),
    rust: impl(
      p(R, ['permissions.rs', 'permissions/**']),
      p(RT, ['permissions_*_tests.rs', 'cli_permissions_tests.rs']),
    ),
    javascript: serverOnly,
  },
  {
    id: 'union-links',
    title: 'Union of several link networks',
    csharp: impl(p(C, ['UnionLinksDecorator.cs']), p(CT, ['UnionAndRemapLinksTests.cs'])),
    rust: impl(p(R, ['union_links.rs']), p(RT, ['union_and_remap_links_tests.rs'])),
    javascript: impl(
      [`${J}union.js`, `${J}composed.js`],
      [`${JT}union-remap.test.mjs`, `${JT}composition-edges.test.mjs`],
    ),
  },
  {
    id: 'remap-links',
    title: 'Address remapping and external ids',
    csharp: impl(
      p(C, ['RemapLinksDecorator.cs', 'AddressMaps.cs', 'LinksExternalIds.cs']),
      p(CT, ['UnionAndRemapLinksTests.cs']),
    ),
    rust: impl(
      p(R, ['remap_links.rs', 'external_ids.rs']),
      p(RT, ['union_and_remap_links_tests.rs']),
    ),
    javascript: impl([`${J}remap.js`, `${J}external-ids.js`], [`${JT}union-remap.test.mjs`]),
  },
  {
    id: 'cache-links',
    title: 'Read cache decorator',
    csharp: impl(p(C, ['CacheLinksDecorator.cs']), p(CT, ['CacheLinksDecoratorTests.cs'])),
    rust: impl(p(R, ['cache_links.rs']), p(RT, ['cache_links_tests.rs'])),
    javascript: impl([`${J}cache.js`], [`${JT}cache.test.mjs`]),
  },
  {
    id: 'doublets-storage',
    title: 'Doublets storage, uniqueness resolver and decorator plumbing',
    csharp: impl(
      p(C, [
        'SimpleLinksDecorator.cs',
        'LinksExtensions.cs',
        'LinksFacadeDisposer.cs',
        'LinksUniquenessAndUsagesRepointingResolver.cs',
        'Exceptions.cs',
      ]),
      p(CT, [
        'SimpleLinksDecoratorTests.cs',
        'LinksUniquenessAndUsagesRepointingResolverTests.cs',
        'Decorator*.cs',
        'MappedGrowthTests.cs',
      ]),
    ),
    rust: impl(
      p(R, ['link_storage.rs', 'link_storage_doublets.rs', 'indexed_links.rs', 'storage/**']),
      p(RT, [
        'doublets_*_tests.rs',
        'link_storage_tests.rs',
        'indexed_links_tests.rs',
        'mapped_*_tests.rs',
        'storage_*_tests.rs',
      ]),
    ),
    javascript: viaWasm,
  },
  {
    id: 'protocol',
    title: 'LinksOperation protocol: client, server, sessions, subscriptions and store archives',
    csharp: impl(
      p(C, [
        'Protocol/Links*.cs',
        'Protocol/LinoProtocol*.cs',
        'Protocol/RemoteLinks*.cs',
        'Protocol/StoreArchive.cs',
      ]),
      p(CT, ['Protocol/*.cs', 'ProtocolEdgeTests.cs']),
    ),
    rust: impl(
      p(R, [
        'protocol/archive.rs',
        'protocol/client.rs',
        'protocol/error.rs',
        'protocol/links_operations.rs',
        'protocol/mod.rs',
        'protocol/protocols.rs',
        'protocol/remote_links.rs',
        'protocol/server.rs',
        'protocol/server/**',
        'protocol/server_query.rs',
        'protocol/sessions.rs',
      ]),
      p(RT, [
        'protocol_*_tests.rs',
        'remote_links_tests.rs',
        'links_sessions_tests.rs',
        'store_archive_*tests.rs',
        'subscriptions_tests.rs',
        'checked_sessions_tests.rs',
      ]),
    ),
    javascript: impl(
      p(J, [
        'client.js',
        'framing.js',
        'lino.js',
        'operations.js',
        'tcp.js',
        'process.js',
        'errors.js',
        'deadline.js',
      ]),
      p(JT, [
        'client.test.mjs',
        'framing.test.mjs',
        'lino.test.mjs',
        'tcp*.test.mjs',
        'process.test.mjs',
        'protocol-edge.test.mjs',
        'utf8-preservation.test.mjs',
      ]),
    ),
  },
  {
    id: 'ipc-transports',
    title: 'Local IPC transports (pipes, sockets, queues, rings, datagrams)',
    csharp: impl(
      p(C, ['Ipc/**']),
      p(CT, [
        'Protocol/IpcTransportTests.cs',
        'Protocol/LinksServerIpcTests.cs',
        'CliIpcEndpointsTests.cs',
      ]),
    ),
    rust: impl(p(R, ['protocol/ipc/**']), p(RT, ['protocol_ipc_tests.rs', 'cli_ipc_tests.rs'])),
    javascript: impl(p(J, ['ipc*.js']), p(JT, ['ipc*.test.mjs'])),
  },
  {
    id: 'graphql',
    title: 'GraphQL server and DeepClient compatibility',
    csharp: impl(p(CLI, ['Graphql/**']), p(CT, ['Graphql*.cs'])),
    rust: impl(p(R, ['graphql/**']), p(RT, ['graphql_*.rs'])),
    javascript: na(
      'The GraphQL endpoint is served by the C# and Rust CLIs; JavaScript DeepClient consumers call it over HTTP (examples/deep-client).',
    ),
  },
  {
    id: 'cli',
    title: 'clink command line: arguments, serve, stop signals and diagnostics',
    csharp: impl(
      p(CLI, ['Program.cs', 'CliIo.cs', 'CliServe.cs', 'StopSignals.cs']).concat([
        `${C}DiagnosticOutput.cs`,
      ]),
      p(CT, ['Cli*.cs', 'Diagnostic*.cs']),
    ),
    rust: impl(
      p(R, ['cli.rs', 'main.rs', 'diagnostics.rs', 'error.rs', 'lib.rs']),
      p(RT, ['cli_*_tests.rs', 'diagnostic_*.rs']),
    ),
    javascript: {
      status: 'delegated',
      to: 'rust',
      via: ['js/src/**', 'rust/wasm/src/lib.rs'],
      tests: ['js/test/linkGraph.test.mjs', 'rust/wasm/tests/web.rs'],
    },
  },
  {
    id: 'engine-core',
    title:
      'Staged engine: sessions, rows, journal, commit logs, savepoints, branches, group commit',
    csharp: impl(
      p(C, [
        'Engine/LinksEngine.cs',
        'Engine/Engine{Session,Rows,Journal,Savepoint,Fork,QueryReservations}.cs',
        'Engine/{QueryReservations,QueryAddressScope,QueryMatching,QueryPreview}.cs',
        'Engine/{CommitLog,DeltaCommitLog,TimedCommitLog,GroupCommit,PessimisticWrites,ConstraintIndex}.cs',
        'Engine/{ImmutableObjectCache,BackgroundCompaction,ChangeRevisionJournal,RevisionJournal}.cs',
        'Engine/{JournalMaintenance,SortedRunRows}.cs',
      ]),
      p(CT, [
        'Engine*.cs',
        'DeltaCommitLogTests.cs',
        'TimedCommitLogTests.cs',
        'PessimisticEngineTests.cs',
        'SortedRunRowsTests.cs',
        'JournalLiveTailTests.cs',
      ]),
    ),
    rust: impl(
      p(R, [
        'engine/mod.rs',
        'engine/{rows,journal,savepoint,branch,constraint_index,pessimistic}.rs',
        'engine/{query_reservation,query_scratch,commit,delta,timed_log,group,object_cache}.rs',
        'engine/{background_compaction,changes,transaction,validation,recovery,sorted_runs,named}.rs',
        'engine/{replication,maintenance}.rs',
        'engine/*tests.rs',
      ]),
      p(RT, ['engine_*_tests.rs']),
    ),
    javascript: impl(
      p(J, [
        'engine.js',
        'engine-delta.js',
        'engine-timed-log.js',
        'engine-logs.js',
        'engine-file-log.js',
        'engine-lock-wait.js',
        'index.js',
        'capabilities.js',
      ]),
      p(JT, [
        'engine.test.mjs',
        'engine-delta.test.mjs',
        'engine-logs.test.mjs',
        'engine-compaction*.test.mjs',
        'conformance/*.mjs',
      ]),
    ),
  },
  {
    id: 'engine-causal',
    title: 'Causal sessions, batches, replicas, projections and Merkle summaries',
    csharp: impl(p(C, ['Engine/Causal*.cs']), p(CT, ['Causal*.cs'])),
    rust: impl(p(R, ['engine/causal*.rs']), p(RT, ['engine_causal_*_tests.rs'])),
    javascript: impl(p(J, ['engine-causal-*.js']), p(JT, ['engine-causal-*.test.mjs'])),
  },
  {
    id: 'engine-native-pages',
    title: 'Native page format, checkpoints and mirrored page devices',
    csharp: impl(
      p(C, [
        'Engine/Checkpoint*.cs',
        'Engine/FileCheckpointPagedCommitLog.cs',
        'Engine/*PageDevice.cs',
        'Engine/JournalCheckpoint.cs',
        'Engine/MirroredCheckpointRoot.cs',
        'Engine/PageFormat.cs',
        'Engine/PagedCommitLog.cs',
      ]),
      p(CT, ['Checkpoint*.cs', 'Paged*.cs', 'Mirrored*.cs', 'NativeCheckpoint*.cs']),
    ),
    rust: impl(
      p(R, [
        'engine/checkpoint.rs',
        'engine/paged/{checkpoint,checkpoint_format,checkpoint_inspect,checkpoint_root}.rs',
        'engine/paged/{file,file_checkpoint,file_checkpoint_process_tests,file_checkpoint_tests,file_events}.rs',
        'engine/paged/{format,mirrored,mod}.rs',
      ]),
      p(RT, [
        'engine_checkpoint*_tests.rs',
        'engine_paged*_tests.rs',
        'engine_mirrored*_tests.rs',
        'engine_file_checkpoint*_tests.rs',
      ]),
    ),
    javascript: impl(
      p(J, [
        'engine-checkpoint-*.js',
        'engine-file-checkpoint-pages.js',
        'engine-file-mirrored-pages.js',
        'engine-file-paged-log.js',
        'engine-mirrored-pages.js',
        'engine-page-*.js',
        'engine-paged-log.js',
      ]),
      p(JT, [
        'engine-checkpoint-*.test.mjs',
        'engine-file-checkpoint-*.test.mjs',
        'engine-*mirrored-pages.test.mjs',
        'engine-*paged-*.test.mjs',
      ]),
    ),
  },
  {
    id: 'engine-native-lsm',
    title: 'Native LSM write-ahead log, sorted tables and read-only inspection',
    csharp: impl(
      p(C, [
        'Engine/FileLsm*.cs',
        'Engine/LsmWal.cs',
        'Engine/NativeLsm*.cs',
        'Engine/PhysicalLsm*.cs',
        'Engine/SortedJournalTable.cs',
      ]),
      p(CT, ['LsmWalTests.cs', 'NativeLsm*.cs', 'PhysicalLsm*.cs', 'SortedJournalTableTests.cs']),
    ),
    rust: impl(
      p(R, ['engine/lsm.rs', 'engine/lsm/**', 'engine/paged/sorted_table.rs']),
      p(RT, ['engine_lsm_*_tests.rs', 'engine_native_lsm_tests.rs']),
    ),
    javascript: impl(
      p(J, ['engine-lsm-*.js', 'engine-file-lsm-*.js', 'engine-sorted-table.js']),
      p(JT, [
        'engine-lsm-*.test.mjs',
        'engine-file-lsm-*.test.mjs',
        'engine-sorted-table.test.mjs',
      ]),
    ),
  },
  {
    id: 'engine-cluster',
    title: 'Raft cluster, etcd journal store and partitioned journals',
    csharp: impl(
      p(C, [
        'Engine/ClusterEngine.cs',
        'Engine/RaftNode.cs',
        'Engine/Etcd*.cs',
        'Engine/PartitionedJournalStore.cs',
      ]),
      p(CT, [
        'EngineCluster*.cs',
        'RaftNodeTests.cs',
        'Etcd*.cs',
        'PartitionedJournalStoreTests.cs',
        'PartitionedObjectCache*.cs',
      ]),
    ),
    rust: impl(
      p(R, [
        'engine/cluster.rs',
        'engine/raft.rs',
        'engine/etcd.rs',
        'engine/etcd/**',
        'engine/partitioned.rs',
      ]),
      p(RT, [
        'engine_cluster_tests.rs',
        'engine_raft_simulation_tests.rs',
        'engine_etcd_*tests.rs',
        'engine_partitioned_tests.rs',
      ]),
    ),
    javascript: impl(
      p(J, ['cluster.js', 'cluster-maintenance.js', 'raft.js', 'partitioned.js']),
      p(JT, [
        'cluster*.test.mjs',
        'raft.test.mjs',
        'engine-raft-simulation.test.mjs',
        'partitioned*.test.mjs',
      ]),
    ),
  },
  {
    id: 'engine-reclamation',
    title: 'Journal reclamation and chunked journals',
    csharp: impl(
      p(C, ['Engine/Reclaim*.cs', 'Engine/Reclamation*.cs', 'Engine/JournalChunks.cs']),
      p(CT, ['Reclamation*.cs', 'PartitionedJournalChunk*.cs']),
    ),
    rust: impl(
      p(R, ['engine/reclaiming.rs', 'engine/reclaiming/**', 'engine/partitioned/chunks.rs']),
      p(RT, ['engine_reclaiming_tests.rs', 'engine_chunked_tests.rs']),
    ),
    javascript: impl(
      p(J, ['reclaiming*.js', 'journal-chunks.js']),
      p(JT, ['reclaiming*.test.mjs', 'partitioned-chunks.test.mjs']),
    ),
  },
  {
    id: 'engine-sqlite',
    title: 'SQLite commit log',
    csharp: impl(
      p(C, ['Engine/SqliteCommitLog.cs']),
      p(CT, ['SqliteCommitLogTests.cs', 'EngineSqliteExitTests.cs']),
    ),
    rust: impl(p(R, ['engine/sqlite.rs']), p(RT, ['engine_sqlite_*tests.rs'])),
    javascript: impl(p(J, ['engine-sqlite-log.js']), p(JT, ['engine-sqlite-log.test.mjs'])),
  },
  {
    id: 'engine-author-fence',
    title: 'Author fences for concurrent writers',
    csharp: impl(p(C, ['Engine/AuthorFence.cs']), p(CT, ['AuthorFence*.cs'])),
    rust: impl(p(R, ['engine/author_fence.rs']), p(RT, ['engine_author_fence_tests.rs'])),
    javascript: impl(p(J, ['engine-author-fence.js']), p(JT, ['engine-author-fence.test.mjs'])),
  },
  {
    id: 'engine-address-allocator',
    title: 'Engine address allocation',
    csharp: impl(p(C, ['Engine/AddressAllocator.cs']), p(CT, ['AddressAllocatorTests.cs'])),
    rust: impl(p(R, ['engine/address_allocator.rs']), p(RT, ['engine_address_allocator_tests.rs'])),
    javascript: impl(
      p(J, ['engine-address-allocator.js']),
      p(JT, ['engine-address-allocator.test.mjs']),
    ),
  },
  {
    id: 'webassembly-bindings',
    title: 'WebAssembly bindings of the Rust core (Clink, staged engine, causal sessions)',
    csharp: na(
      '.NET consumers reference the C# library directly; the WebAssembly build exists for browsers and Node.js.',
    ),
    rust: impl(['rust/wasm/src/**'], ['rust/wasm/tests/*.rs']),
    javascript: impl([`${J}wasm.js`], [`${JT}wasm.test.mjs`]),
  },
];

const manifest = {
  $comment:
    'Cross-language parity manifest (issue #111). Run `node parity/check-parity.mjs --help` text in the file header; docs/PARITY.md explains the rules.',
  schema: 1,
  languages: {
    csharp: { roots: [`${C}**/*.cs`, `${CLI}**/*.cs`] },
    rust: { roots: ['rust/src/**/*.rs', 'rust/wasm/src/**/*.rs'] },
    javascript: { roots: ['js/src/**', 'js/client/src/**/*.js'] },
  },
  shared: [],
  features: features.map((feature) => ({ requirements: undefined, ...feature })),
};
writeFileSync(
  new URL('../../parity/features.json', import.meta.url),
  `${JSON.stringify(manifest, null, 2)}\n`,
);
