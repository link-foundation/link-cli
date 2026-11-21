// The serving side of rust/src/protocol/server.rs: the job queue between the
// connections and the serving loop (`Job`, the `mpsc` channel), the
// `Dispatcher` that runs every request against the sessions, the
// subscriptions of a `Connection` and the `Outbox` of documents waiting to be
// written to one connection.
//
// Rust runs every connection on two threads and the requests on the thread of
// `serve`; here they are tasks of one event loop. A request still runs to
// completion, synchronously, before the next one starts, so the store is
// never touched by two requests at once.
import { NULL_LINK, isNull, link } from './link.js';
import {
  LinksOperation,
  matches,
  notificationDocument,
  subscriptionDocument,
} from './protocol-links-operations.js';
import { admitDocument, formatDocument } from './protocol-lino.js';
import { diagnostics } from './protocol-errors.js';
import {
  TripletsOperation,
  matchesTriplet,
  packTriplets,
  tripletNotificationDocument,
} from './protocol-triplets-operations.js';
import { tripletChange } from './triplets.js';

/** The default of `ServerOptions.maxPendingNotifications`. */
export const DEFAULT_MAX_PENDING_NOTIFICATIONS = 1024;

const U32_LIMIT = 2 ** 32;

/** Wakes every waiter at once: the `Condvar` of the Rust outbox. */
export class Wakeup {
  #waiters = [];

  wait() {
    return new Promise((resolve) => this.#waiters.push(resolve));
  }

  notifyAll() {
    const waiters = this.#waiters;
    this.#waiters = [];
    for (const wake of waiters) wake();
  }
}

/**
 * The channel of jobs (`mpsc`): `send` is false once the receiving side is
 * gone, and closing it fails the `done` of every request still queued, as
 * dropping the Rust receiver drops their senders.
 */
export class JobQueue {
  #jobs = [];
  #closed = false;
  #arrived = new Wakeup();

  /** Queues `job`; false when the queue is closed. */
  send(job) {
    if (this.#closed) return false;
    this.#jobs.push(job);
    this.#arrived.notifyAll();
    return true;
  }

  /**
   * The next job; `SHUTDOWN` once the queue is closed, as the Rust `recv`
   * fails once every sender is gone and `serve` returns.
   */
  async recv() {
    while (this.#jobs.length === 0 && !this.#closed) await this.#arrived.wait();
    if (this.#closed) return SHUTDOWN;
    return this.#jobs.shift();
  }

  /** Stops the queue; every queued request is told it will not run. */
  close() {
    if (this.#closed) return;
    this.#closed = true;
    const jobs = this.#jobs;
    this.#jobs = [];
    for (const job of jobs) if (job.kind === 'request') job.done(false);
    this.#arrived.notifyAll();
  }

  isClosed() {
    return this.#closed;
  }
}

/** A request of `connection`; `done(true)` is called once its reply is queued. */
export const requestJob = (connection, outbox, document, format, done) => ({
  kind: 'request',
  connection,
  outbox,
  document,
  format,
  done,
});

/** `connection` hung up: its subscriptions go with it. */
export const closeJob = (connection) => ({ kind: 'close', connection });

export const SHUTDOWN = Object.freeze({ kind: 'shutdown' });

/**
 * The documents waiting to be written to one connection, replies and pushes
 * in one queue, so a push never interleaves with a reply and a slow
 * subscriber never holds up the store.
 */
export class Outbox {
  constructor(maxPendingNotifications = DEFAULT_MAX_PENDING_NOTIFICATIONS, hangUp = () => {}) {
    this.maxPendingNotifications = maxPendingNotifications;
    this.hangUp = hangUp;
    /** `[document, format, isReply]` triples, in the order they are written. */
    this.queue = [];
    this.pendingReplies = 0;
    this.pendingNotifications = 0;
    this.closed = false;
    this.changed = new Wakeup();
  }

  /** Queues a reply; `waitForReplies` waits until it is written. */
  reply(document, format) {
    if (this.closed) return;
    this.queue.push([document, format, true]);
    this.pendingReplies += 1;
    this.changed.notifyAll();
  }

  /**
   * Queues a push; closes the connection instead when too many are already
   * waiting. False once the outbox is closed.
   */
  push(document, format) {
    if (this.closed) return false;
    if (this.pendingNotifications < this.maxPendingNotifications) {
      this.queue.push([document, format, false]);
      this.pendingNotifications += 1;
      this.changed.notifyAll();
      return true;
    }
    this.close();
    return false;
  }

  /** Resolves once every queued reply is written, or the outbox closed. */
  async waitForReplies() {
    while (this.pendingReplies > 0 && !this.closed) await this.changed.wait();
  }

  isClosed() {
    return this.closed;
  }

  /** Writes the queue to `stream`, in order, until the outbox closes. */
  async writeAll(stream, limits) {
    try {
      await this.#tryWriteAll(stream, limits);
    } catch (error) {
      // The connection broke: its reader stops too.
      this.close();
      throw error;
    }
  }

  async #tryWriteAll(stream, limits) {
    for (;;) {
      while (this.queue.length === 0 && !this.closed) await this.changed.wait();
      if (this.closed) return;
      const [document, format] = this.queue[0];
      await format.protocol(limits).writeDocument(stream, document);
      const written = this.queue.shift();
      if (written !== undefined) {
        if (written[2]) this.pendingReplies -= 1;
        else this.pendingNotifications -= 1;
      }
      this.changed.notifyAll();
    }
  }

  /** Stops the outbox and hangs up; called again, it does nothing. */
  close() {
    if (this.closed) return;
    this.closed = true;
    this.queue = [];
    this.changed.notifyAll();
    this.hangUp();
  }
}

/**
 * The subscriptions of one connection, and the session its requests run as;
 * only the serving loop touches them.
 */
export class Connection {
  constructor(outbox, session) {
    this.outbox = outbox;
    this.session = session;
    /** Subscription number → `[restriction, format]`, in ascending order. */
    this.subscriptions = new Map();
    this.nextSubscription = 1;
  }

  /**
   * The reply to a `subscribe` or an `unsubscribe`; null for any other
   * request. Subscription numbers start at 1 on every connection.
   */
  execute(document, format, triplets) {
    if (triplets) {
      let operation;
      try {
        operation = TripletsOperation.fromDocument(document);
      } catch {
        return null;
      }
      if (operation !== null) {
        return operation.kind === 'subscribe'
          ? this.subscribe(operation.restriction, format)
          : null;
      }
    }
    let operation;
    try {
      operation = LinksOperation.fromDocument(document);
    } catch {
      return null;
    }
    if (operation?.kind === 'subscribe') return this.subscribe(operation.restriction, format);
    if (operation?.kind === 'unsubscribe') {
      // An unknown number is ignored.
      this.subscriptions.delete(operation.index);
      return [];
    }
    return null;
  }

  subscribe(restriction, format) {
    const subscription = this.nextSubscription;
    this.nextSubscription = (this.nextSubscription + 1) % U32_LIMIT;
    this.subscriptions.set(subscription, [restriction, format]);
    return subscriptionDocument(subscription);
  }

  /** The subscriptions in ascending order, as the Rust `BTreeMap` iterates. */
  #ordered() {
    return [...this.subscriptions].sort(([a], [b]) => a - b);
  }

  /**
   * Pushes `changes` to every subscription they fit, one `(notify: N)`
   * document each; a side that does not fit (or that `readable` refuses) is
   * left out, and a change left with no side is not pushed.
   */
  notify(changes, readable) {
    for (const [subscription, [restriction, format]] of this.#ordered()) {
      const side = (value) =>
        !isNull(value) && matches(value, restriction) && readable(value) ? value : NULL_LINK;
      const seen = changes
        .map(([before, after]) => [side(before), side(after)])
        .filter(([before, after]) => !isNull(before) || !isNull(after));
      if (seen.length > 0 && !this.outbox.push(notificationDocument(subscription, seen), format)) {
        return;
      }
    }
  }

  /** `notify` for the changes of a server in triplet mode. */
  notifyTriplets(changes, readable) {
    if (changes.length === 0) return;
    for (const [subscription, [restriction, format]] of this.#ordered()) {
      const side = (value) =>
        value !== null &&
        matchesTriplet(value, restriction) &&
        readable(link(value.index, value.typeId, value.index))
          ? value
          : null;
      const seen = changes
        .map((change) => tripletChange(side(change.before), side(change.after)))
        .filter((change) => change.before !== null || change.after !== null);
      if (
        seen.length > 0 &&
        !this.outbox.push(
          packTriplets(tripletNotificationDocument(subscription, seen), format),
          format,
        )
      ) {
        return;
      }
    }
  }
}

/** Runs the jobs of a server and keeps the subscriptions of every connection. */
export class Dispatcher {
  constructor(options, readable = null) {
    this.options = options;
    this.readable = readable;
    /** Connection number → `Connection`. */
    this.connections = new Map();
  }

  /** Runs `job`; false once the server should stop. */
  run(sessions, job) {
    if (job.kind === 'shutdown') return false;
    if (job.kind === 'close') {
      const closed = this.connections.get(job.connection);
      if (closed !== undefined) {
        this.connections.delete(job.connection);
        sessions.close(closed.session);
      }
      return true;
    }
    const { connection, outbox, document, format, done } = job;
    if (this.options.trace && admitted(sessions.store(), document)) {
      diagnostics.stderr(`[server] request: ${formatDocument(document)}`);
    }
    let current = this.connections.get(connection);
    if (current === undefined) {
      current = new Connection(outbox, sessions.open(null));
      this.connections.set(connection, current);
    }
    const triplets = sessions.hasTriplets();
    const changes = [];
    const tripletChanges = [];
    let reply = current.execute(document, format, triplets);
    if (reply === null) {
      reply = triplets
        ? sessions.executeTripletsObserved(current.session, document, tripletChanges)
        : sessions.executeObserved(current.session, document, changes);
    }
    if (this.options.trace) diagnostics.stderr(`[server] reply: ${formatDocument(reply)}`);
    current.outbox.reply(triplets ? packTriplets(reply, format) : reply, format);
    if (changes.length > 0 || tripletChanges.length > 0) {
      const readable = this.readable;
      for (const id of [...this.connections.keys()].sort((a, b) => a - b)) {
        if (id === connection) continue;
        const other = this.connections.get(id);
        const canRead = (value) =>
          (readable === null || readable(value)) && sessions.canRead(other.session, value);
        other.notify(changes, canRead);
        other.notifyTriplets(tripletChanges, canRead);
      }
    }
    done(true);
    return true;
  }

  /** Closes the session of every connection still open. */
  closeAll(sessions) {
    const connections = this.connections;
    this.connections = new Map();
    for (const closed of connections.values()) sessions.close(closed.session);
  }
}

/** Whether a request may be traced: a store without limits traces all of them. */
export function admitted(store, document) {
  const limits = store.queryLimits();
  if (limits === null) return true;
  try {
    admitDocument(document, limits);
    return true;
  } catch {
    return false;
  }
}
