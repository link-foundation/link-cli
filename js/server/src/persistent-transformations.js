// Persistent transformation triggers, the port of
// rust/src/persistent_transformations.rs (itself the port of
// `Foundation.Data.Doublets.Cli.PersistentTransformationDecorator`).
//
// A *trigger* is a stored LiNo substitution query that the CLI replays after
// every write, turning a one-off transformation into a standing rule. It keeps
// the on-disk shape of the C# and Rust tools, so all of them read each other's
// trigger databases:
//
//   (Always ((Condition <condition text>) (Substitution <substitution text>)))
//   (Once   ((Condition <condition text>) (Substitution <substitution text>)))
//
// `Condition`, `Substitution`, `Type`, `Trigger`, `Once` and `Always` are
// named points; the condition and substitution texts are named points too,
// whose names carry `INTERNAL_NAME_PREFIX` so they cannot collide with
// user-visible names.
//
// `TriggerStore` decides where the triggers live: a sidecar store
// (`<db>.triggers.links` by default, see `makeTriggersDatabaseFilename`) or
// the decorated database itself, the `--embed-triggers` mode.
import { NamedTypeLinks } from './named-type-links.js';
import { formatIdentifier, parse } from './query-parser.js';
import { QueryProcessor } from './query-processor.js';

/**
 * Prefix of the internal names of stored condition and substitution texts,
 * which keeps trigger bookkeeping apart from user names even when both share
 * one namespace.
 */
export const INTERNAL_NAME_PREFIX = '__persistent_transformation:';

const MISSING_PARTS =
  'Persistent transformation query must contain a condition and a substitution.';

/** How long a stored trigger lives: `Once` is removed after an application that changed something. */
export const PersistentTransformationKind = Object.freeze({ Once: 'Once', Always: 'Always' });

/** A trigger as it is stored: its root `(kind payload)` link, kind and halves. */
export class PersistentTransformation {
  constructor(root, kind, condition, substitution) {
    this.root = root;
    this.kind = kind;
    this.condition = condition;
    this.substitution = substitution;
  }

  /** The query that gets replayed after every write. */
  query() {
    return `(${this.condition} ${this.substitution})`;
  }
}

/**
 * Renders a parsed LiNo link back to source text: a link without values is
 * its (escaped) identifier, one without an identifier is `(values)`, one with
 * both `(id: values)`.
 */
function formatLino(value) {
  const values = value.values ?? [];
  const id = value.id ?? '';
  if (values.length === 0) return id === '' ? '()' : formatIdentifier(id);
  const rendered = values.map(formatLino).join(' ');
  return id === '' ? `(${rendered})` : `(${formatIdentifier(id)}: ${rendered})`;
}

/**
 * A trigger query split into its condition and substitution halves, both
 * re-formatted from the parse tree, so two spellings of one query store as
 * one trigger.
 */
export class PersistentTransformationQuery {
  constructor(condition, substitution) {
    this.condition = condition;
    this.substitution = substitution;
  }

  /** Parses the wrapped form `((condition) (substitution))` or the bare `(condition) (substitution)`. */
  static parse(query) {
    const parsed = parse(query);
    if (parsed.length === 0) throw new Error(MISSING_PARTS);
    const outer = parsed[0];
    let parts;
    if (outer.values != null && outer.values.length >= 2) parts = outer.values;
    else if (parsed.length >= 2) parts = parsed;
    else throw new Error(MISSING_PARTS);
    return new PersistentTransformationQuery(formatLino(parts[0]), formatLino(parts[1]));
  }

  /** The normalised `(condition substitution)` query text. */
  query() {
    return `(${this.condition} ${this.substitution})`;
  }
}

/**
 * The components of a Unix path as Rust's `Path::components` reads them, each
 * with the offset its text ends at: a root, a leading `.`, `..` and names.
 */
function pathComponents(path) {
  const components = [];
  let position = 0;
  if (path.startsWith('/')) {
    components.push({ kind: 'root', end: 1 });
    position = 1;
  }
  while (position < path.length) {
    let end = path.indexOf('/', position);
    if (end === -1) end = path.length;
    const text = path.slice(position, end);
    if (text === '.' && components.length === 0) components.push({ kind: 'current', end });
    else if (text === '..') components.push({ kind: 'parent', end });
    else if (text !== '' && text !== '.') components.push({ kind: 'name', text, end });
    position = end + 1;
  }
  return components;
}

/** Rust's `Path::file_stem`, `''` when there is none. */
function fileStem(last) {
  if (last == null || last.kind !== 'name') return '';
  const dot = last.text.lastIndexOf('.');
  return dot <= 0 ? last.text : last.text.slice(0, dot);
}

/**
 * The conventional sidecar of a database: `<stem>.triggers.links` next to it,
 * with Rust's `Path::file_stem` and `Path::parent`.
 */
export function makeTriggersDatabaseFilename(databaseFilename) {
  const components = pathComponents(databaseFilename);
  const last = components.at(-1);
  const name = `${fileStem(last)}.triggers.links`;
  if (last == null || last.kind === 'root' || components.length === 1) return name;
  const parent = databaseFilename.slice(0, components.at(-2).end);
  return parent.endsWith('/') ? `${parent}${name}` : `${parent}/${name}`;
}

/** Where a `PersistentTransformationDecorator` keeps its triggers. */
export class TriggerStore {
  constructor(store) {
    /** The companion store, or `null` for the decorated database itself. */
    this.store = store;
  }

  /** In the decorated database itself (`--embed-triggers`). */
  static embedded() {
    return new TriggerStore(null);
  }

  /** In `store`, a companion database (the default). */
  static sidecar(store) {
    return new TriggerStore(store);
  }

  isEmbedded() {
    return this.store == null;
  }
}

const SCHEMA_NAMES = ['Type', 'Trigger', 'Once', 'Always', 'Condition', 'Substitution'];

/** Creates the schema points and the links that relate them, and returns them. */
function ensureSchema(links) {
  const [type, trigger, once, always, condition, substitution] = SCHEMA_NAMES.map((name) =>
    links.getOrCreateNamed(name),
  );
  links.tryGetOrCreate(type, trigger);
  links.tryGetOrCreate(trigger, once);
  links.tryGetOrCreate(trigger, always);
  links.tryGetOrCreate(type, condition);
  links.tryGetOrCreate(type, substitution);
  return { once, always, condition, substitution };
}

/** Reads the schema without creating anything; `null` when any of its points is missing. */
function tryGetSchema(links) {
  const ids = [];
  for (const name of SCHEMA_NAMES) {
    const id = links.getByName(name);
    if (id == null) return null;
    ids.push(id);
  }
  return { once: ids[2], always: ids[3], condition: ids[4], substitution: ids[5] };
}

const conditionTextName = (condition) => `${INTERNAL_NAME_PREFIX}condition:${condition}`;
const substitutionTextName = (substitution) =>
  `${INTERNAL_NAME_PREFIX}substitution:${substitution}`;

function decodeTextName(name, part) {
  const prefix = `${INTERNAL_NAME_PREFIX}${part}:`;
  return name != null && name.startsWith(prefix) ? name.slice(prefix.length) : null;
}

/** Every well-formed trigger in `links`, ordered by root address. */
export function triggersIn(links) {
  const schema = tryGetSchema(links);
  if (schema == null) return [];
  const all = [...links.allLinks()].sort((left, right) => left.index - right.index);
  const byIndex = new Map(all.map((value) => [value.index, value]));
  const triggers = [];
  for (const value of all) {
    let kind;
    if (value.source === schema.always) kind = PersistentTransformationKind.Always;
    else if (value.source === schema.once) kind = PersistentTransformationKind.Once;
    else continue;
    const payload = byIndex.get(value.target);
    if (payload == null) continue;
    const conditionRecord = byIndex.get(payload.source);
    const substitutionRecord = byIndex.get(payload.target);
    if (conditionRecord == null || substitutionRecord == null) continue;
    if (
      conditionRecord.source !== schema.condition ||
      substitutionRecord.source !== schema.substitution
    ) {
      continue;
    }
    const condition = decodeTextName(links.getName(conditionRecord.target), 'condition');
    const substitution = decodeTextName(links.getName(substitutionRecord.target), 'substitution');
    if (condition == null || substitution == null) continue;
    triggers.push(new PersistentTransformation(value.index, kind, condition, substitution));
  }
  return triggers;
}

/** Writes `parsed` into `links` as a trigger of `kind`, idempotently; returns its root. */
export function storeTriggerIn(links, kind, parsed) {
  const schema = ensureSchema(links);
  const conditionText = links.getOrCreateNamed(conditionTextName(parsed.condition));
  const substitutionText = links.getOrCreateNamed(substitutionTextName(parsed.substitution));
  const conditionRecord = links.tryGetOrCreate(schema.condition, conditionText);
  const substitutionRecord = links.tryGetOrCreate(schema.substitution, substitutionText);
  const payload = links.tryGetOrCreate(conditionRecord, substitutionRecord);
  const triggerType = kind === PersistentTransformationKind.Always ? schema.always : schema.once;
  return links.tryGetOrCreate(triggerType, payload);
}

/** Deletes the `(kind payload)` root link, leaving the shared schema and texts in place. */
export function deleteTriggerRoot(links, root) {
  if (!links.exists(root)) return false;
  links.delete(root);
  return true;
}

/**
 * Replays stored triggers after every write that goes through it; wrap it
 * around any `NamedTypeLinks`.
 *
 * `create`, `ensureCreated` and `getOrCreate` are infallible in Rust, so a
 * trigger failing after one is parked and surfaced by the next fallible
 * operation -- at the latest by `save`, which the CLI always calls.
 */
export class PersistentTransformationDecorator extends NamedTypeLinks {
  #links;
  #triggers;
  #trace;
  #stdout;
  #stderr;
  #applyingTriggers = false;
  #suppressTriggers = false;
  #autoCreateMissingReferences = false;
  #pendingError = null;

  /**
   * `options.stdout` receives the trace lines of the decorator (default:
   * process stdout); `options.stderr(line)`, those of the replayed queries
   * (default: the query processor's, process stderr).
   */
  constructor(links, triggers, trace, { stdout = process.stdout, stderr } = {}) {
    super();
    this.#links = links;
    this.#triggers = triggers;
    this.#trace = trace;
    this.#stdout = stdout;
    this.#stderr = stderr;
  }

  /** Keeps the triggers in the decorated database itself. */
  static embedded(links, trace, options) {
    return new PersistentTransformationDecorator(links, TriggerStore.embedded(), trace, options);
  }

  /** Keeps the triggers in `triggerLinks`. */
  static withSidecar(links, triggerLinks, trace, options) {
    const store = TriggerStore.sidecar(triggerLinks);
    return new PersistentTransformationDecorator(links, store, trace, options);
  }

  /** Whether replayed triggers may create missing references as points. */
  withAutoCreateMissingReferences(enabled) {
    this.#autoCreateMissingReferences = enabled;
    return this;
  }

  autoCreateMissingReferences() {
    return this.#autoCreateMissingReferences;
  }

  setAutoCreateMissingReferences(enabled) {
    this.#autoCreateMissingReferences = enabled;
  }

  inner() {
    return this.#links;
  }

  triggerStore() {
    return this.#triggers;
  }

  /** The store that holds the triggers. */
  #triggerLinks() {
    return this.#triggers.store ?? this.#links;
  }

  /** Stores `query` as a trigger of `kind` and returns its root address. */
  storeTrigger(kind, query) {
    const parsed = PersistentTransformationQuery.parse(query);
    const root = this.#withoutTriggerApplication(() =>
      storeTriggerIn(this.#triggerLinks(), kind, parsed),
    );
    this.#traceMsg(`Stored ${kind} trigger #${root}: ${parsed.query()}`);
    return root;
  }

  /** Removes every stored trigger whose query equals `query`; how many were removed. */
  removeTriggers(query) {
    const parsed = PersistentTransformationQuery.parse(query);
    return this.#withoutTriggerApplication(() => {
      const matching = this.triggers()
        .filter(
          (trigger) =>
            trigger.condition === parsed.condition && trigger.substitution === parsed.substitution,
        )
        .map((trigger) => trigger.root);
      for (const root of matching) {
        deleteTriggerRoot(this.#triggerLinks(), root);
        this.#traceMsg(`Deleted trigger #${root}`);
      }
      return matching.length;
    });
  }

  /** Every stored trigger, ordered by root address. */
  triggers() {
    return triggersIn(this.#triggerLinks());
  }

  /**
   * Runs `action` with trigger application suppressed, so bookkeeping does not
   * trigger itself. As in Rust, a failing `action` leaves them suppressed.
   */
  #withoutTriggerApplication(action) {
    const previous = this.#suppressTriggers;
    this.#suppressTriggers = true;
    const result = action();
    this.#suppressTriggers = previous;
    return result;
  }

  /** Parks a trigger failure raised by an infallible write. */
  #afterWrite() {
    try {
      this.#applyTriggersAfterOperation();
    } catch (error) {
      if (this.#pendingError == null) this.#pendingError = error;
    }
  }

  /** Throws (and clears) a failure parked by `#afterWrite`. */
  #takePendingError() {
    const error = this.#pendingError;
    this.#pendingError = null;
    if (error != null) throw error;
  }

  #applyTriggersAfterOperation() {
    if (this.#suppressTriggers || this.#applyingTriggers) return;
    const triggers = this.triggers();
    if (triggers.length === 0) return;
    this.#applyingTriggers = true;
    try {
      this.#applyTriggers(triggers);
    } finally {
      this.#applyingTriggers = false;
    }
  }

  #applyTriggers(triggers) {
    const processor = new QueryProcessor(this.#trace, {
      stderr: this.#stderr,
    }).withAutoCreateMissingReferences(this.#autoCreateMissingReferences);
    for (const trigger of triggers) {
      const changes = processor.processQuery(this, trigger.query());
      if (changes.length === 0 || trigger.kind !== PersistentTransformationKind.Once) continue;
      const root = trigger.root;
      this.#withoutTriggerApplication(() => deleteTriggerRoot(this.#triggerLinks(), root));
      this.#traceMsg(`Deleted trigger #${root}`);
    }
  }

  #traceMsg(message) {
    if (this.#trace) this.#stdout.write(`[PersistentTransformation] ${message}\n`);
  }

  tryCreate(source, target) {
    const index = this.#links.tryCreate(source, target);
    this.#applyTriggersAfterOperation();
    this.#takePendingError();
    return index;
  }

  tryEnsureCreated(id) {
    const existed = this.#links.exists(id);
    const index = this.#links.tryEnsureCreated(id);
    if (!existed) this.#applyTriggersAfterOperation();
    this.#takePendingError();
    return index;
  }

  tryGetOrCreate(source, target) {
    const existed = this.#links.search(source, target) != null;
    const index = this.#links.tryGetOrCreate(source, target);
    if (!existed) this.#applyTriggersAfterOperation();
    this.#takePendingError();
    return index;
  }

  create(source, target) {
    const index = this.#links.create(source, target);
    this.#afterWrite();
    return index;
  }

  /**
   * Only a call that really allocates a link counts as a write, as in C#,
   * where `EnsureCreated` of an existing address performs no operation.
   */
  ensureCreated(id) {
    const existed = this.#links.exists(id);
    const index = this.#links.ensureCreated(id);
    if (!existed) this.#afterWrite();
    return index;
  }

  getLink(id) {
    return this.#links.getLink(id);
  }

  exists(id) {
    return this.#links.exists(id);
  }

  updateObserved(id, source, target, observer) {
    const value = this.#links.updateObserved(id, source, target, observer);
    this.#applyTriggersAfterOperation();
    this.#takePendingError();
    return value;
  }

  deleteObserved(id, observer) {
    const value = this.#links.deleteObserved(id, observer);
    this.#applyTriggersAfterOperation();
    this.#takePendingError();
    return value;
  }

  allLinks() {
    return this.#links.allLinks();
  }

  queryLinks(index = null, source = null, target = null) {
    return this.#links.queryLinks(index, source, target);
  }

  search(source, target) {
    return this.#links.search(source, target);
  }

  /** Fires triggers only when the pair had to be created, as `ensureCreated`. */
  getOrCreate(source, target) {
    const found = this.#links.search(source, target);
    if (found != null) return found;
    const index = this.#links.getOrCreate(source, target);
    this.#afterWrite();
    return index;
  }

  getName(id) {
    return this.#links.getName(id);
  }

  setName(id, name) {
    return this.#links.setName(id, name);
  }

  getByName(name) {
    return this.#links.getByName(name);
  }

  removeName(id) {
    return this.#links.removeName(id);
  }

  save() {
    this.#takePendingError();
    this.#links.save();
    if (this.#triggers.store != null) this.#triggers.store.save();
  }
}
