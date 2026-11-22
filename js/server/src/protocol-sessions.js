// Sessions of a links server, each with its subject: ports
// rust/src/protocol/sessions.rs (`LinksSessions`, the authenticator of
// `(auth: token)`, the atomic requests under permissions).
//
// `LinksSessions` is what a `LinksServer` gives every connection, without the
// network. Without permissions every session sees and changes the whole
// store. With them (`withPermissions`) a session sees the store through a
// `PermissionsDecorator` of its subject, and every request is atomic: one that
// fails is undone whole. Requests run one at a time; every method is
// synchronous, as the store is.
//
// A session id is a number. The authenticator is `(store, token) => subject |
// null`; a thrown error fails the authentication.
import { simplifyChanges } from './changes-simplifier.js';
import { NULL_LINK, sameLink } from './link.js';
import { alternateText } from './protocol-errors.js';
import { LinksOperation } from './protocol-links-operations.js';
import {
  errorDocument,
  errorMessage,
  formatDocument,
  isRef,
  parseDocument,
  parseDocumentWithLimits,
  admitDocument,
} from './protocol-lino.js';
import { executeObservedRequest, executeRequest } from './protocol-server-query.js';
import {
  TripletsOperation,
  executeObservedTripletsRequest,
} from './protocol-triplets-operations.js';
import { PermissionsDecorator } from './permissions-named-links.js';
import { QueryProcessor } from './query-processor.js';
import { TripletMarkers, TripletsOverDoublets } from './triplets.js';
import { TripletsPermissionsDecorator } from './triplets-permissions.js';
import { UndoableLinksDecorator } from './undoable-links.js';

/** A store served to many sessions, each with its subject. */
export class LinksSessions {
  #plain = null;
  #view = null;
  #processor = new QueryProcessor(false);
  #triplets = null;
  #authenticate = null;
  #subject = 0;
  #sessions = new Map();
  #next = 0;

  /** Sessions that see and change the whole store. */
  constructor(store) {
    this.#plain = store;
  }

  /** Sessions that see the store as `policy` lets their subjects. */
  static withPermissions(store, policy) {
    const sessions = new LinksSessions(null);
    sessions.#view = new PermissionsDecorator(new UndoableLinksDecorator(store), policy, 0);
    return sessions;
  }

  /** Answers `(auth: token)` with `authenticate(store, token)`. */
  withAuthenticator(authenticate) {
    this.#authenticate = authenticate;
    return this;
  }

  /** The subject of a session before any `(auth: token)`; 0, the default, is a guest. */
  withSubject(subject) {
    this.#subject = subject;
    return this;
  }

  /** Passed to `QueryProcessor.withAutoCreateMissingReferences`. */
  withAutoCreateMissingReferences(enabled) {
    this.#processor = new QueryProcessor(false).withAutoCreateMissingReferences(enabled);
    return this;
  }

  /** Reads every request as triplets, creating the markers of the encoding when missing. */
  withTriplets() {
    this.ensureTriplets();
    return this;
  }

  /** Like `withTriplets`, on sessions already built; returns the markers. */
  ensureTriplets() {
    if (this.#triplets === null) this.#triplets = TripletMarkers.ensure(this.store());
    return this.#triplets;
  }

  hasTriplets() {
    return this.#triplets !== null;
  }

  hasPermissions() {
    return this.#view !== null;
  }

  /** The store, below any permissions. */
  store() {
    return this.#view === null ? this.#plain : this.#view.inner().inner();
  }

  /** The policy, when there are permissions; null otherwise. */
  policy() {
    return this.#view === null ? null : this.#view.policy();
  }

  /** Opens a session of `subject`, by default the one of `withSubject`. */
  open(subject = null) {
    const id = this.#next;
    this.#next += 1;
    this.#sessions.set(id, { subject: subject ?? this.#subject, created: new Set() });
    return id;
  }

  close(session) {
    this.#sessions.delete(session);
  }

  /** The sessions that are open, in order. */
  sessions() {
    return [...this.#sessions.keys()].sort((a, b) => a - b);
  }

  /** The subject the requests of `session` run as, while it is open; null otherwise. */
  subject(session) {
    return this.#sessions.get(session)?.subject ?? null;
  }

  /** The links `session` created, while they are its own; null when it is not open. */
  created(session) {
    return this.#sessions.get(session)?.created ?? null;
  }

  /** Answers a request written as LiNo text, as the text protocol does. */
  request(session, text) {
    const limits = this.store().queryLimits();
    let document;
    try {
      document = limits === null ? parseDocument(text) : parseDocumentWithLimits(text, limits);
    } catch (error) {
      return formatDocument(errorDocument(error.message));
    }
    return formatDocument(this.execute(session, document));
  }

  /** Answers one request document of `session`; a session that is not open is opened. */
  execute(session, document) {
    return this.executeObserved(session, document, []);
  }

  /** Like `execute`, adding the net `[before, after]` change of every changed link to `changed`. */
  executeObserved(session, document, changed) {
    return this.#run(session, document, changed, []);
  }

  /** Like `execute` in triplet mode, adding every changed triplet to `changed`. */
  executeTripletsObserved(session, document, changed) {
    return this.#run(session, document, [], changed);
  }

  #run(session, document, changed, tripletsChanged) {
    const limits = this.store().queryLimits();
    if (limits !== null) {
      try {
        admitDocument(document, limits);
      } catch (error) {
        return errorDocument(error.message);
      }
    }
    const state = this.#sessions.get(session) ?? { subject: this.#subject, created: new Set() };
    this.#sessions.delete(session);
    try {
      return this.#executeAs(state, document, changed, tripletsChanged);
    } finally {
      this.#sessions.set(session, state);
    }
  }

  /** Whether the subject of `session` may read `link`: always, without permissions. */
  canRead(session, link) {
    const view = this.#view;
    if (view === null) return true;
    const state = this.#sessions.get(session);
    view.setSubject(state === undefined ? this.#subject : state.subject);
    const created = view.replaceCreated(state === undefined ? new Set() : state.created);
    const readable = this.#triplets !== null ? view.canReadTriplet(link) : view.canRead(link);
    const restored = view.replaceCreated(created);
    if (state !== undefined) state.created = restored;
    return readable;
  }

  #executeAs(session, document, changed, tripletsChanged) {
    const [first] = document;
    if (document.length === 1 && !isRef(first) && first.id === 'auth') {
      return this.#authenticateAs(session, first.values);
    }
    const markers = this.#triplets;
    const view = this.#view;
    if (view === null) {
      if (markers !== null) {
        const triplets = TripletsOverDoublets.withMarkers(this.#plain, markers);
        return executeObservedTripletsRequest(triplets, document, tripletsChanged);
      }
      return executeObservedRequest(this.#plain, this.#processor, document, changed);
    }
    view.setSubject(session.subject);
    view.replaceCreated(session.created);
    session.created = new Set();
    view.inner().begin();
    const observed = tripletsChanged.length;
    let reply;
    try {
      reply =
        markers !== null
          ? tripletsRequest(view, markers, document, tripletsChanged)
          : request(view, document, (target) => executeRequest(target, this.#processor, document));
    } catch (panicked) {
      try {
        undo(view);
      } catch {
        // Rust ignores a failed undo while a panic unwinds.
      }
      session.created = view.replaceCreated(new Set());
      throw panicked;
    }
    const message = errorMessage(reply);
    if (message === null) {
      const log = view.inner().log().changes();
      const deleted = new Set();
      for (const [before, after] of log) {
        if (before != null && after == null) deleted.add(before.index);
      }
      const changes = simplifyChanges(
        log.map(([before, after]) => [before ?? NULL_LINK, after ?? NULL_LINK]),
      );
      changed.push(...changes.filter(([before, after]) => !sameLink(before, after)));
      view.inner().commit();
      // The links are gone: no other session owns them any more.
      for (const other of this.#sessions.values()) {
        for (const index of deleted) other.created.delete(index);
      }
    } else {
      tripletsChanged.length = observed;
      try {
        undo(view);
      } catch (error) {
        reply = errorDocument(`${message}; undoing it failed: ${alternateText(error)}`);
      }
    }
    session.created = view.replaceCreated(new Set());
    return reply;
  }

  #authenticateAs(session, values) {
    let found = null;
    if (values.length === 1 && isRef(values[0]) && this.#authenticate !== null) {
      try {
        found = this.#authenticate(this.store(), values[0]) ?? null;
      } catch {
        found = null;
      }
    }
    if (found === null) return errorDocument('Forbidden: authentication failed');
    session.subject = found;
    return [{ id: 'subject', values: [String(found)] }];
  }
}

/** Undoes the request, and forgets the links it created. */
function undo(view) {
  for (const created of view.inner().rollback()) view.forget(created);
}

/**
 * A request as the subject sees the store: names of links it cannot read are
 * not found, and `run` answers everything else.
 */
function request(view, document, run) {
  let operation;
  try {
    operation = LinksOperation.fromDocument(document);
  } catch (error) {
    return errorDocument(error.message);
  }
  if (operation?.kind === 'get-name' && !view.exists(operation.index)) return [];
  if (operation?.kind === 'get-by-name') {
    let found;
    try {
      found = view.getByName(operation.name);
    } catch (error) {
      return errorDocument(alternateText(error));
    }
    return found != null && found !== 0 && view.exists(found) ? run(view) : [];
  }
  return run(view);
}

/**
 * A request over triplets as the subject sees the store: the permissions move
 * above the triplets, which write their wrappers and pairs straight to the
 * store below, still inside the transaction of the request.
 */
function tripletsRequest(view, markers, document, changed) {
  const subject = view.subject();
  const created = view.replaceCreated(new Set());
  const [links, policy] = view.partsMut();
  const layer = new TripletsPermissionsDecorator(
    TripletsOverDoublets.withMarkers(links, markers),
    policy,
    subject,
  ).withCreated(created);
  // A triplet operation holds four parts where one over doublets holds three:
  // only what is left, the names, is for `request` to look at.
  let plain;
  try {
    plain = TripletsOperation.fromDocument(document) === null;
  } catch {
    plain = false;
  }
  const run = (target) => executeObservedTripletsRequest(target, document, changed);
  const reply = plain ? request(layer, document, run) : run(layer);
  view.replaceCreated(layer.replaceCreated(new Set()));
  return reply;
}
