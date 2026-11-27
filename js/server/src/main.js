// The `clink` entry point, the port of rust/src/main.rs: parse the command
// line, then serve (`--serve`, `--forward`, `--graphql`), act as a client
// (`--connect`), or open the store -- bare, in transactions or under version
// control -- and run the query pipeline over it, with the trigger, token and
// permission stages around it.
//
// `main(args, io)` returns the exit status instead of exiting, so it can run
// in-process; bin/clink.js exits with it. A failure prints `Error: ` and its
// cause chain on standard error with status 1, as the Rust `main` returning
// an `anyhow::Error` does.
import { existsSync } from 'node:fs';

import { Cli } from './cli.js';
import { loadEnvironmentFiles } from './env-files.js';
import { reportError } from './error-report.js';
import { setTrace } from './ipc.js';
import { importLinoFile } from './lino-database-input.js';
import { runGraphqlServer } from './main-graphql.js';
import {
  finish as finishWithTriggers,
  openSideStore,
  resolveSubject,
  runPermittedQuery,
  runTokens,
  triggerCommandCount,
} from './main-permissions.js';
import { runClient, runRelay, runServer } from './main-protocol.js';
import {
  parseCommitMode,
  parseRetention,
  runWithTransactions,
  runWithVc,
} from './main-transactions.js';
import { NamedTypesDecorator } from './named-types.js';
import { openTransitionsJournal } from './transactions-journal-file.js';
import { PermissionPolicy } from './permissions.js';
import { exportStoreFile, importStoreFile } from './protocol-store-file.js';
import { QueryProcessor } from './query-processor.js';
import { rustTrim } from './query-parser.js';
import { LockMode } from './storage-lock.js';
import { lockOrder, lockStatus, parseStoreLockOptions, StoreLock } from './store-lock.js';

/** `NamedTypesDecorator::new`, with the lookup indexes of `--indexed-lookups` or `--graphql`. */
export function openNamedStorage(path, cli, io = {}) {
  const storage = NamedTypesDecorator.open(path, {
    trace: cli.trace,
    stderr: io.stderr,
  });
  return cli.indexedLookups || cli.graphql ? storage.withIndexedLookups() : storage;
}

/** Imports the store archive of `--import-binary`, then the LiNo file of `--in`. */
export function importInputs(cli, storage) {
  if (cli.binaryInput != null) importStoreFile(storage, cli.binaryInput);
  if (cli.linoInput != null) {
    importLinoFile(storage, cli.linoInput, { maxDepth: cli.linoInputMaxDepth });
  }
}

/** Writes the whole store to the LiNo file of `--out` and the store archive of `--export-binary`. */
export function exportOutputs(cli, storage) {
  if (cli.linoOutput != null) storage.writeLinoOutput(cli.linoOutput);
  if (cli.binaryOutput != null) exportStoreFile(storage, cli.binaryOutput);
}

/**
 * The query pipeline, with a hook that may consume the query before it
 * reaches the `QueryProcessor`: `triggerStage(cli, storage, query)` runs after
 * `--structure` and before the query is processed; `true` ends the run after
 * the outputs are written.
 */
export function runQueryPipelineWith(cli, storage, triggerStage, io = {}) {
  const out = io.stdout ?? process.stdout;
  const err = io.stderr ?? process.stderr;
  if (cli.before) storage.printAllLino(out);
  importInputs(cli, storage);
  if (cli.structure != null) {
    out.write(`${storage.formatStructure(cli.structure)}\n`);
    exportOutputs(cli, storage);
    return;
  }
  const query = cli.query ?? cli.queryArg;
  if (triggerStage(cli, storage, query)) {
    exportOutputs(cli, storage);
    return;
  }
  let changes = [];
  if (query != null && query !== '') {
    const processor = new QueryProcessor(cli.trace, {
      stderr: (message) => err.write(`${message}\n`),
    }).withAutoCreateMissingReferences(cli.autoCreateMissingReferences);
    changes = processor.processQuery(storage, query);
  }
  if (cli.changes) {
    for (const [before, after] of changes) storage.printChangeLino(before, after, out);
  }
  if (cli.after) storage.printAllLino(out);
  exportOutputs(cli, storage);
}

const nonEmpty = (text) => (text == null || rustTrim(text) === '' ? null : rustTrim(text));

/**
 * Runs the invocation over `storage`: the token commands, the server, or the
 * query pipeline, as the subject sees the store when `--permissions` is given.
 */
export async function run(cli, storage, triggerStage, io = {}) {
  const out = io.stdout ?? process.stdout;
  const subject = resolveSubject(cli, storage);
  const open = (path) => openNamedStorage(path, cli, io);
  const tokensSource = nonEmpty(cli.tokens);
  let tokens;
  if (tokensSource != null) tokens = { store: openSideStore(cli, tokensSource, open) };
  else if (cli.grantToken != null || cli.revokeToken != null) {
    throw new Error('--grant-token and --revoke-token need --tokens.');
  }
  let policy = null;
  const permissionsSource = nonEmpty(cli.permissions);
  if (permissionsSource != null) {
    // Triggers run below the view, as the store itself.
    if (triggerCommandCount(cli) > 0) {
      throw new Error('--always, --once and --never cannot be used with --permissions.');
    }
    const rules = openSideStore(cli, permissionsSource, open);
    policy =
      rules == null ? PermissionPolicy.inData(storage) : PermissionPolicy.withRules(storage, rules);
  }

  if (cli.grantToken != null || cli.revokeToken != null) {
    runTokens(cli, storage, tokens.store, subject, out);
    return;
  }
  if (cli.serve.length > 0 && cli.graphql) {
    await runGraphqlServer(cli, cli.serve[0], storage, policy, io);
    return;
  }
  if (cli.serve.length > 0) {
    await runServer(cli, storage, policy, subject, tokens, io);
    return;
  }
  const pipeline = (pipelineCli, view, stage) => runQueryPipelineWith(pipelineCli, view, stage, io);
  if (policy == null) pipeline(cli, storage, triggerStage);
  else runPermittedQuery(cli, storage, policy, subject, pipeline);
  storage.save();
}

/** Opens the sidecar store of the triggers and runs the invocation over `storage`. */
export function finish(cli, storage, io = {}) {
  return finishWithTriggers(cli, storage, {
    run: (runCli, runStorage, stage) => run(runCli, runStorage, stage, io),
    openTriggerStore: (path, trace) => NamedTypesDecorator.open(path, { trace, stderr: io.stderr }),
    out: io.stdout ?? process.stdout,
  });
}

/** Opens the store of `--db` and runs the invocation over it. */
export function runBare(cli, io = {}) {
  return finish(cli, openNamedStorage(cli.db, cli, io), io);
}

/**
 * The store files this run may save: the database (whose lock covers its
 * default side stores, `<db>.*.links` and the `<db>.transitions.log` journal)
 * and every side store named explicitly.
 */
export function lockedStores(cli) {
  const stores = [cli.db];
  for (const side of [cli.transactionsFile, cli.vcFile, cli.triggersFile]) {
    if (side != null) stores.push(side);
  }
  for (const source of [cli.tokens, cli.permissions]) {
    const path = source == null ? '' : rustTrim(source);
    if (path !== '' && path !== 'self') stores.push(path);
  }
  return stores;
}

/**
 * Read-only runs share the lock; anything that may write takes it alone, and
 * so does a first run on a store that does not exist yet, since opening it
 * creates its files.
 */
export function lockMode(cli) {
  const query = cli.query ?? cli.queryArg;
  const readOnly =
    existsSync(cli.db) &&
    (query == null || query === '') &&
    cli.linoInput == null &&
    cli.binaryInput == null &&
    triggerCommandCount(cli) === 0 &&
    cli.serve.length === 0 &&
    cli.grantToken == null &&
    cli.revokeToken == null &&
    !cli.transactionsRequested();
  return readOnly ? LockMode.Shared : LockMode.Exclusive;
}

/** Runs `clink` with `args` (without the program name) and resolves its exit status. */
export async function main(args, io = {}) {
  const out = io.stdout ?? process.stdout;
  const err = io.stderr ?? process.stderr;
  try {
    // `Cli::parse` runs `lino_arguments::init()` first: `.lenv`, then `.env`.
    loadEnvironmentFiles({ env: io.env ?? process.env, cwd: io.cwd });
    const parsed = Cli.parse(args);
    if (parsed.kind === 'help') {
      out.write(Cli.helpText());
      return 0;
    }
    if (parsed.kind === 'version') {
      out.write(`${Cli.versionText()}\n`);
      return 0;
    }
    const { cli } = parsed;
    if (triggerCommandCount(cli) > 1) {
      throw new Error('Only one of --always, --once, or --never can be used at a time.');
    }
    if (cli.serve.length > 0 && cli.connect != null) {
      throw new Error('--serve and --connect cannot be used together.');
    }
    if (cli.trace) setTrace(true);
    if (cli.forward != null) {
      await runRelay(cli, cli.forward, io);
      return 0;
    }
    if (cli.connect != null) {
      if (cli.indexedLookups) {
        throw new Error('--indexed-lookups applies to local stores; enable it on the server.');
      }
      await runClient(cli, cli.connect, io);
      return 0;
    }
    const commitMode = parseCommitMode(cli.commitMode);
    const retention = parseRetention(cli.retention);
    const lockOptions = parseStoreLockOptions(cli.lock, cli.lockTimeout);
    const stores = lockOrder(lockedStores(cli));
    if (cli.lockStatus) {
      out.write(lockStatus(stores));
      return 0;
    }
    // Held until the stores are saved: the whole read-modify-write (#194).
    const lock = await StoreLock.acquire(stores, lockMode(cli), {
      ...lockOptions,
      trace: cli.trace,
      stderr: err,
    });
    try {
      await runLocked(cli, commitMode, retention, io);
    } finally {
      lock.release();
    }
    return 0;
  } catch (error) {
    reportError(error, err);
    return 1;
  }
}

/** Opens the store -- bare, in transactions or under version control -- and runs the invocation. */
async function runLocked(cli, commitMode, retention, io) {
  const err = io.stderr ?? process.stderr;
  const context = {
    finish: (finishCli, storage) => finish(finishCli, storage, io),
    openNamedStorage: (path, openCli) => openNamedStorage(path, openCli, io),
    openJournal: (database, explicit) =>
      openTransitionsJournal(database, explicit, { trace: cli.trace, stderr: err }),
    out: io.stdout ?? process.stdout,
    err,
  };
  if (cli.vcRequested()) await runWithVc(cli, commitMode, retention, context);
  else if (cli.transactionsRequested())
    await runWithTransactions(cli, commitMode, retention, context);
  else await runBare(cli, io);
}
