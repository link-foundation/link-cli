// The permission, token and trigger stages of the `clink` entry point, the
// port of `triggers_path`, `persistent_transformations_enabled`, `finish`,
// `open_side_store`, `resolve_subject`, `run_tokens`, `write_tokens`,
// `run_permitted_query` and `run_trigger_command` of rust/src/main.rs.
//
// The stages owned elsewhere -- `run`, the query pipeline and opening a named
// store -- come in as callbacks, so this module stays below them. `cli` holds
// the camelCase fields of rust/src/cli.rs; `out` is where the stages print
// (default: process stdout).
import { existsSync } from 'node:fs';
import { errorChain, errorText } from './error-report.js';
import { LinkError, QueryError } from './errors.js';
import { LinksTokenTable, PermissionsDecorator } from './permissions.js';
import {
  makeTriggersDatabaseFilename,
  PersistentTransformationDecorator,
  PersistentTransformationKind,
  TriggerStore,
} from './persistent-transformations.js';
import { rustTrim } from './query-parser.js';
import { UndoableLinksDecorator } from './undoable-links.js';

/** The no-op trigger stage: the query goes on to the processor. */
const noTriggerStage = () => false;

/** How many of `--always`, `--once` and `--never` were given (`Cli::trigger_command_count`). */
export function triggerCommandCount(cli) {
  return Number(Boolean(cli.always)) + Number(Boolean(cli.once)) + Number(Boolean(cli.never));
}

/** Whether any flag of the persistent transformation family was given (`Cli::persistent_transformations_requested`). */
export function persistentTransformationsRequested(cli) {
  return Boolean(
    cli.always ||
    cli.once ||
    cli.never ||
    cli.triggers ||
    cli.embedTriggers ||
    cli.triggersFile != null,
  );
}

/** The trigger sidecar store: `--triggers-file` when given, `<db>.triggers.links` otherwise. */
export function triggersPath(cli) {
  return cli.triggersFile != null ? cli.triggersFile : makeTriggersDatabaseFilename(cli.db);
}

/**
 * Whether triggers run: any flag of their family turns the layer on, and so
 * does an existing trigger store, so triggers stored earlier keep firing.
 */
export function persistentTransformationsEnabled(cli) {
  return persistentTransformationsRequested(cli) || existsSync(triggersPath(cli));
}

/**
 * Runs the invocation over `storage` with `run(cli, storage, triggerStage)`,
 * wrapping it in a `PersistentTransformationDecorator` first when triggers
 * are enabled, so the decorator always sits outermost. `openTriggerStore(path,
 * trace)` opens the sidecar store (`NamedTypesDecorator::new` in Rust).
 */
export function finish(cli, storage, { run, openTriggerStore, out = process.stdout }) {
  if (!persistentTransformationsEnabled(cli)) return run(cli, storage, noTriggerStage);
  const triggerStore = cli.embedTriggers
    ? TriggerStore.embedded()
    : TriggerStore.sidecar(openTriggerStore(triggersPath(cli), cli.trace));
  const decorated = new PersistentTransformationDecorator(storage, triggerStore, cli.trace, {
    stdout: out,
  }).withAutoCreateMissingReferences(cli.autoCreateMissingReferences);
  return run(cli, decorated, (stageCli, stageStorage, query) =>
    runTriggerCommand(stageCli, stageStorage, query, out),
  );
}

/**
 * The store a `--tokens` or `--permissions` value names: the data itself
 * (`self`, `null` here) or a links file of its own, opened by
 * `openNamedStorage(path, cli)`.
 */
export function openSideStore(cli, source, openNamedStorage) {
  return source === 'self' ? null : openNamedStorage(source, cli);
}

/** `--subject`: a link index, or the name of a link; 0 (a guest) when not given. */
export function resolveSubject(cli, storage) {
  const text = cli.subject == null ? '' : rustTrim(cli.subject);
  if (text === '') return 0;
  if (/^[0-9]+$/.test(text)) {
    const subject = Number(text);
    if (subject <= 0xffffffff) return subject;
  }
  const subject = storage.getByName(text);
  if (subject == null || subject === 0) throw new Error(`Unknown subject '${text}'.`);
  return subject;
}

/**
 * `--grant-token` and `--revoke-token`: writes the token table of
 * `--tokens`, in `tokens` or in the data when `null`.
 */
export function runTokens(cli, storage, tokens, subject, out = process.stdout) {
  if (cli.grantToken != null && subject === 0) throw new Error('--grant-token needs a --subject.');
  if (tokens != null) {
    writeTokens(cli, tokens, subject, out);
    tokens.save();
  } else {
    writeTokens(cli, storage, subject, out);
  }
  // The data holds the points the token table names, even with a table of its own.
  storage.save();
}

/** Grants and revokes the tokens of `cli` in the table stored in `tokens`. */
export function writeTokens(cli, tokens, subject, out = process.stdout) {
  const table = new LinksTokenTable(tokens);
  if (cli.grantToken != null) {
    const granted = table.grant(cli.grantToken, subject);
    out.write(`Token granted to subject ${subject}: ${granted}\n`);
  }
  if (cli.revokeToken != null) {
    out.write(table.revoke(cli.revokeToken) ? 'Token revoked.\n' : 'Unknown token.\n');
  }
}

/** Whether `error` or a cause of it is a `LinkError` (anyhow's `downcast_ref` through contexts). */
function isLinkError(error) {
  return errorChain(error).some((cause) => cause instanceof LinkError);
}

/**
 * The query pipeline, `runQueryPipelineWith(cli, storage, triggerStage)`, as
 * `subject` sees the store under `policy`. The query is atomic: a failed one
 * is undone whole, even where the store changed links the subject cannot
 * see. A failure that is no `LinkError` is reported as a query error.
 */
export function runPermittedQuery(cli, storage, policy, subject, runQueryPipelineWith) {
  const undoable = new UndoableLinksDecorator(storage);
  const view = new PermissionsDecorator(undoable, policy, subject);
  undoable.begin();
  try {
    runQueryPipelineWith(cli, view, noTriggerStage);
  } catch (error) {
    undoable.rollback();
    if (isLinkError(error)) throw error;
    throw new QueryError(errorChain(error).map(errorText).join(': '));
  }
  undoable.commit();
}

/**
 * Handles `--always`, `--once` and `--never` over `storage`, a
 * `PersistentTransformationDecorator`. Returns `true` when the query was
 * taken as a trigger definition and must not run against the database.
 */
export function runTriggerCommand(cli, storage, query, out = process.stdout) {
  if (triggerCommandCount(cli) === 0) return false;
  if (query == null || rustTrim(query) === '') {
    throw new Error('--always, --once, and --never require a query.');
  }
  if (cli.always || cli.once) {
    const kind = cli.always
      ? PersistentTransformationKind.Always
      : PersistentTransformationKind.Once;
    const root = storage.storeTrigger(kind, query);
    out.write(`${kind} persistent transformation trigger stored: ${root}\n`);
  } else {
    const removed = storage.removeTriggers(query);
    out.write(`Persistent transformation triggers removed: ${removed}\n`);
  }
  return true;
}
