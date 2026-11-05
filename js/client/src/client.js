// The links client: one stable contract over interchangeable transports, the
// JS counterpart of `INamedTypesLinks` / `NamedTypeLinks` behind which the C#
// and Rust `RemoteLinks` swap a local store for a server.

import { TRIPLET_OPERATIONS } from './capabilities.js';
import { requireGuarantees } from './composed.js';
import { scopeError, scopeOf } from './deadline.js';
import { DeadlineExceededError, LinksError, MalformedMessageError, RemoteError, UnsupportedError } from './errors.js';
import { formatDocument, parseDocument } from './lino.js';
import { errorMessage, parseNotification, parseTripletNotification, replies, requests } from './operations.js';

/**
 * A links store behind a transport. Every address it takes or returns is a
 * BigInt (safe-integer numbers are accepted on input). Every method takes
 * `{ signal, deadline, timeout }`; a failed call rejects with a
 * {@link LinksError} whose `outcome` says whether the request may have run.
 *
 * A transport is `{ capabilities, send(text, { signal, deadline }), close() }`
 * where `send` resolves to the LiNo text of the reply. A transport that
 * receives pushes (TCP) also has `receivePush(scope)`, `onPush(listener)` and
 * `pendingPushes`, which subscriptions need.
 */
export class LinksClient {
  constructor(transport, { capabilities } = {}) {
    if (!transport || typeof transport.send !== 'function') throw new TypeError('a transport has a send(text, options) method');
    this.transport = transport;
    this.capabilities = Object.freeze({ ...transport.capabilities, ...capabilities });
  }

  /** Throws an UnsupportedError unless the backend offers every guarantee named. */
  require(...guarantees) {
    requireGuarantees(this.capabilities, guarantees);
    return this;
  }

  /** Sends LiNo text as it is and resolves to the reply text, `(error: ...)` replies included. */
  async requestText(text, options = {}) {
    const scope = scopeOf(options);
    const early = scopeError(scope, 'not-sent');
    if (early) throw early;
    return this.transport.send(text, scope);
  }

  /** Sends a document and resolves to the reply document; `(error: ...)` rejects with a RemoteError. */
  async request(document, options = {}) {
    const reply = parseDocument(await this.requestText(formatDocument(document), options));
    const message = errorMessage(reply);
    if (message !== null) throw new RemoteError(message);
    return reply;
  }

  get maxAddress() {
    return this.capabilities.maxAddress ?? null;
  }

  /** The number of links matching a restriction (see `operations.js`). */
  async count(restriction = [], options) {
    return replies.count(await this.request(requests.count(restriction, this.maxAddress), options));
  }

  /** Every link matching a restriction, by address, as `{ index, source, target }`. */
  async each(restriction = [], options) {
    return replies.links(await this.request(requests.each(restriction, this.maxAddress), options));
  }

  /**
   * Creates `(source target)` and resolves to the link created, or to null
   * when the backend created none because it keeps links unique and an equal
   * one exists (`capabilities.duplicateLinks === false`, e.g. the C# server).
   */
  async create(source, target, options) {
    return createdLink(await this.request(requests.create(source, target, this.maxAddress), options));
  }

  /** Updates a link; resolves to the net change of every link touched. */
  async update(index, source, target, options) {
    return replies.changes(await this.request(requests.update(index, source, target, this.maxAddress), options));
  }

  /** Deletes a link and every link using it; resolves to their changes. */
  async delete(index, options) {
    return replies.changes(await this.request(requests.delete(index, this.maxAddress), options));
  }

  /** The name of a link, or null. */
  async getName(index, options) {
    return replies.name(await this.request(requests.getName(index, this.maxAddress), options));
  }

  /**
   * Names a link. Resolves to the address the backend answers with: where its
   * names store keeps the name, which differs between backends (the file
   * stores answer a link of their names store, the in-memory store the link).
   */
  async setName(index, name, options) {
    return replies.link(await this.request(requests.setName(index, name, this.maxAddress), options));
  }

  /** The link with a name, or null. */
  async getByName(name, options) {
    return replies.link(await this.request(requests.getByName(name), options));
  }

  async removeName(index, options) {
    replies.empty(await this.request(requests.removeName(index, this.maxAddress), options));
  }

  /**
   * Runs a substitution query (`() ((1 1))`, `((1: 1 1)) ()`, ...) and resolves
   * to its changes. Their references are the text the backend wrote: a
   * number, or the name of a named link.
   */
  async query(text, options) {
    if (typeof text !== 'string' || text.trim() === '') throw new TypeError('a query is non-empty LiNo text');
    return replies.namedChanges(await this.request(this.#queryDocument(text), options));
  }

  #queryDocument(text) {
    // Preflight every batch member before sending any writes. Only peers that
    // explicitly advertise this envelope receive the original reference kinds.
    const document = parseDocument(text, this.capabilities.losslessQueries ? this.capabilities.limits : undefined);
    return this.capabilities.losslessQueries ? [{ id: 'query', values: [text] }] : document;
  }

  /** Every link, names substituted for named references (the empty request). */
  async exportLinks(options) {
    return replies.namedLinks(await this.request([], options));
  }

  /**
   * Subscribes to the changes other connections make to the links matching a
   * restriction (the same patterns as `each`) and resolves to the number of
   * the subscription, counted from 1 on every connection. Each later write
   * of another connection that touches a matching link is pushed as one
   * notification per subscription; this connection's own writes are not.
   */
  async subscribe(restriction = [], options) {
    this.#requirePushes();
    return replies.subscription(await this.request(requests.subscribe(restriction, this.maxAddress), options));
  }

  /** Ends a subscription; an unknown number is ignored. Pushes already received stay receivable. */
  async unsubscribe(subscription, options) {
    this.#requirePushes();
    replies.empty(await this.request(requests.unsubscribe(subscription), options));
  }

  /**
   * The next notification, `{ subscription, changes }`, where each change is
   * `{ before, after }` and a side the subscription does not see is null.
   * Waits until one arrives; resolves to null when the `deadline` or
   * `timeout` passes first, rejects with an AbortError when `signal` aborts
   * and with a ConnectionClosedError once the connection is closed.
   */
  async receiveNotification(options = {}) {
    this.#requirePushes();
    let text;
    try {
      text = await this.transport.receivePush(scopeOf(options));
    } catch (error) {
      if (error instanceof DeadlineExceededError) return null;
      throw error;
    }
    return parseNotification(parseDocument(text));
  }

  /**
   * Calls `listener({ subscription, changes })` with every notification from
   * now on; while a listener is registered, notifications are not kept for
   * `receiveNotification`. Returns the function that removes the listener.
   */
  onNotification(listener) {
    this.#requirePushes();
    if (typeof listener !== 'function') throw new TypeError('a notification listener is a function');
    return this.transport.onPush((text) => listener(parseNotification(parseDocument(text))));
  }

  /** The number of notifications received and not taken yet. */
  get pendingNotifications() {
    return this.transport.pendingPushes ?? 0;
  }

  #requirePushes() {
    if (typeof this.transport.receivePush !== 'function' || typeof this.transport.onPush !== 'function') {
      throw new UnsupportedError(`the ${this.capabilities.backend} backend does not push changes: subscriptions need a connection to a server`);
    }
  }

  // The calls below speak to a server in triplet mode (`clink --serve --triplets`,
  // docs/protocol/triplets.txt). Every triplet is `{ index, type, from, to }`.
  // Names and `unsubscribe` are the same calls as above.

  #requireTriplets() {
    if (this.capabilities.triplets === false) {
      throw new UnsupportedError(`the ${this.capabilities.backend} backend does not serve triplets: they need a server started with --triplets`);
    }
  }

  /** The number of triplets matching up to four parts `[index, type, from, to]`. */
  async countTriplets(restriction = [], options) {
    this.#requireTriplets();
    return replies.count(await this.request(requests.countTriplets(restriction, this.maxAddress), options));
  }

  /** Every triplet matching up to four parts `[index, type, from, to]`, by address. */
  async eachTriplets(restriction = [], options) {
    this.#requireTriplets();
    return replies.triplets(await this.request(requests.eachTriplets(restriction, this.maxAddress), options));
  }

  /**
   * Creates a triplet and resolves to it. The type is a triplet; `from` and
   * `to` are triplets or 0. Two triplets may hold the same three parts.
   */
  async createTriplet(type, from, to, options) {
    this.#requireTriplets();
    return createdTriplet(await this.request(requests.createTriplet(type, from, to, this.maxAddress), options));
  }

  /** Creates a point, a triplet that is its own type (`(i: i 0 0)`), and resolves to it. */
  createTripletPoint(options) {
    return this.createTriplet(0n, 0n, 0n, options);
  }

  /** Updates a triplet; resolves to the net change of every triplet touched. */
  async updateTriplet(index, type, from, to, options) {
    this.#requireTriplets();
    return replies.tripletChanges(await this.request(requests.updateTriplet(index, type, from, to, this.maxAddress), options));
  }

  /** Deletes a triplet and every triplet using it; resolves to their changes. */
  async deleteTriplet(index, options) {
    this.#requireTriplets();
    return replies.tripletChanges(await this.request(requests.delete(index, this.maxAddress), options));
  }

  /**
   * Runs a substitution query with three-part links (`() ((1 0 0))`) and
   * resolves to its changes, references as the backend wrote them.
   */
  async queryTriplets(text, options) {
    this.#requireTriplets();
    if (typeof text !== 'string' || text.trim() === '') throw new TypeError('a query is non-empty LiNo text');
    return replies.namedTripletChanges(await this.request(parseDocument(text), options));
  }

  /** Every triplet, references as the backend wrote them (the empty request). */
  async exportTriplets(options) {
    this.#requireTriplets();
    return replies.namedTriplets(await this.request([], options));
  }

  /** `subscribe` with up to four parts; read its pushes with the two calls below. */
  async subscribeTriplets(restriction = [], options) {
    this.#requireTriplets();
    this.#requirePushes();
    return replies.subscription(await this.request(requests.subscribeTriplets(restriction, this.maxAddress), options));
  }

  /** `receiveNotification` with changes of triplets. */
  async receiveTripletNotification(options = {}) {
    this.#requireTriplets();
    this.#requirePushes();
    let text;
    try {
      text = await this.transport.receivePush(scopeOf(options));
    } catch (error) {
      if (error instanceof DeadlineExceededError) return null;
      throw error;
    }
    return parseTripletNotification(parseDocument(text));
  }

  /** `onNotification` with changes of triplets. */
  onTripletNotification(listener) {
    this.#requireTriplets();
    this.#requirePushes();
    if (typeof listener !== 'function') throw new TypeError('a notification listener is a function');
    return this.transport.onPush((text) => listener(parseTripletNotification(parseDocument(text))));
  }

  /**
   * Runs operations as separate requests, up to `window` at a time (pipelined
   * over TCP), and stops sending after the first failure. This is not a
   * transaction: what ran stays applied. The result tells, per operation,
   * `ok` (with `value`), `failed`, `unknown` (sent, outcome not known) or
   * `not-sent`; `completed` is true only when every operation is `ok`.
   *
   * An operation is `{ op, ...arguments }` with `op` one of the client
   * methods: `{ op: 'create', source, target }`, `{ op: 'update', index,
   * source, target }`, `{ op: 'delete', index }`, `{ op: 'setName', index,
   * name }`, `{ op: 'getName', index }`, `{ op: 'getByName', name }`,
   * `{ op: 'removeName', index }`, `{ op: 'count', restriction }`,
   * `{ op: 'each', restriction }`, `{ op: 'query', text }`. A server in triplet
   * mode also takes `{ op: 'countTriplets', restriction }`, `{ op: 'eachTriplets',
   * restriction }`, `{ op: 'createTriplet', type, from, to }`, `{ op:
   * 'updateTriplet', index, type, from, to }`, `{ op: 'deleteTriplet', index }`
   * and `{ op: 'queryTriplets', text }`.
   */
  async batch(operations, { window = 32, signal, deadline, timeout } = {}) {
    if (!Number.isInteger(window) || window < 1) throw new TypeError('window is a positive integer');
    // Validate everything first: a malformed operation sends nothing.
    const calls = operations.map((operation, position) => this.#batchCall(operation, position));
    const scope = scopeOf({ signal, deadline, timeout });
    const results = calls.map(() => ({ status: 'not-sent' }));
    let next = 0;
    let stopped = false;
    const worker = async () => {
      while (!stopped && next < calls.length) {
        const position = next;
        next += 1;
        results[position] = { status: 'unknown' };
        try {
          results[position] = { status: 'ok', value: await calls[position](scope) };
        } catch (error) {
          results[position] = { status: error instanceof LinksError ? error.outcome : 'unknown', error };
          stopped = true;
        }
      }
    };
    await Promise.all(Array.from({ length: Math.min(window, calls.length) }, worker));
    const tally = { ok: 0, failed: 0, unknown: 0, 'not-sent': 0 };
    for (const result of results) tally[result.status] += 1;
    return {
      completed: tally.ok === calls.length,
      results,
      ok: tally.ok,
      failed: tally.failed,
      unknown: tally.unknown,
      notSent: tally['not-sent'],
      firstError: results.find((result) => result.error)?.error ?? null,
    };
  }

  #batchCall(operation, position) {
    const max = this.maxAddress;
    const documentOf = {
      count: ({ restriction = [] }) => [requests.count(restriction, max), replies.count],
      each: ({ restriction = [] }) => [requests.each(restriction, max), replies.links],
      create: ({ source, target }) => [requests.create(source, target, max), createdLink],
      update: ({ index, source, target }) => [requests.update(index, source, target, max), replies.changes],
      delete: ({ index }) => [requests.delete(index, max), replies.changes],
      getName: ({ index }) => [requests.getName(index, max), replies.name],
      setName: ({ index, name }) => [requests.setName(index, name, max), replies.link],
      getByName: ({ name }) => [requests.getByName(name), replies.link],
      removeName: ({ index }) => [requests.removeName(index, max), replies.empty],
      query: ({ text }) => [parseDocument(text), replies.namedChanges],
      countTriplets: ({ restriction = [] }) => [requests.countTriplets(restriction, max), replies.count],
      eachTriplets: ({ restriction = [] }) => [requests.eachTriplets(restriction, max), replies.triplets],
      createTriplet: ({ type, from, to }) => [requests.createTriplet(type, from, to, max), createdTriplet],
      updateTriplet: ({ index, type, from, to }) => [requests.updateTriplet(index, type, from, to, max), replies.tripletChanges],
      deleteTriplet: ({ index }) => [requests.delete(index, max), replies.tripletChanges],
      queryTriplets: ({ text }) => [parseDocument(text), replies.namedTripletChanges],
    };
    const build = documentOf[operation?.op];
    if (!build) throw new TypeError(`operation ${position}: unknown op "${operation?.op}"`);
    let request;
    let parse;
    try {
      if (TRIPLET_OPERATIONS.includes(operation.op)) this.#requireTriplets();
      [request, parse] = build(operation);
    } catch (error) {
      error.message = `operation ${position} (${operation.op}): ${error.message}`;
      throw error;
    }
    return async (scope) => parse(await this.request(request, scope));
  }

  /** Closes what the client owns: its connection, and its server process if it started one. */
  async close() {
    await this.transport.close?.();
  }

  async [Symbol.asyncDispose]() {
    await this.close();
  }
}

/** The link a create reply holds, or null when it holds no change. */
function createdLink(reply) {
  const changes = replies.changes(reply);
  if (changes.length === 0) return null;
  const created = changes.find((change) => change.before === null && change.after !== null);
  if (!created) throw new MalformedMessageError('the create reply holds changes but no created link');
  return created.after;
}

/** The triplet a create reply holds. */
function createdTriplet(reply) {
  const created = replies.tripletChanges(reply).find((change) => change.before === null && change.after !== null);
  if (!created) throw new MalformedMessageError('the create reply holds no created triplet');
  return created.after;
}

/**
 * Forwards every call to another links client: subclass it and override the
 * calls to observe or change, the way the C# and Rust decorators wrap a store.
 * Because a decorator is itself a client, local (WASM) and remote (TCP)
 * stores, and decorators over either, are swapped by swapping one value.
 */
export class LinksDecorator {
  constructor(links) {
    if (!links) throw new TypeError('a decorator wraps a links client');
    this.links = links;
  }

  get capabilities() {
    return this.links.capabilities;
  }

  require(...guarantees) {
    this.links.require(...guarantees);
    return this;
  }

  requestText(text, options) {
    return this.links.requestText(text, options);
  }

  request(document, options) {
    return this.links.request(document, options);
  }

  count(restriction, options) {
    return this.links.count(restriction, options);
  }

  each(restriction, options) {
    return this.links.each(restriction, options);
  }

  create(source, target, options) {
    return this.links.create(source, target, options);
  }

  update(index, source, target, options) {
    return this.links.update(index, source, target, options);
  }

  delete(index, options) {
    return this.links.delete(index, options);
  }

  getName(index, options) {
    return this.links.getName(index, options);
  }

  setName(index, name, options) {
    return this.links.setName(index, name, options);
  }

  getByName(name, options) {
    return this.links.getByName(name, options);
  }

  removeName(index, options) {
    return this.links.removeName(index, options);
  }

  query(text, options) {
    return this.links.query(text, options);
  }

  exportLinks(options) {
    return this.links.exportLinks(options);
  }

  batch(operations, options) {
    return this.links.batch(operations, options);
  }

  subscribe(restriction, options) {
    return this.links.subscribe(restriction, options);
  }

  unsubscribe(subscription, options) {
    return this.links.unsubscribe(subscription, options);
  }

  receiveNotification(options) {
    return this.links.receiveNotification(options);
  }

  onNotification(listener) {
    return this.links.onNotification(listener);
  }

  get pendingNotifications() {
    return this.links.pendingNotifications;
  }

  countTriplets(restriction, options) {
    return this.links.countTriplets(restriction, options);
  }

  eachTriplets(restriction, options) {
    return this.links.eachTriplets(restriction, options);
  }

  createTriplet(type, from, to, options) {
    return this.links.createTriplet(type, from, to, options);
  }

  createTripletPoint(options) {
    return this.links.createTripletPoint(options);
  }

  updateTriplet(index, type, from, to, options) {
    return this.links.updateTriplet(index, type, from, to, options);
  }

  deleteTriplet(index, options) {
    return this.links.deleteTriplet(index, options);
  }

  queryTriplets(text, options) {
    return this.links.queryTriplets(text, options);
  }

  exportTriplets(options) {
    return this.links.exportTriplets(options);
  }

  subscribeTriplets(restriction, options) {
    return this.links.subscribeTriplets(restriction, options);
  }

  receiveTripletNotification(options) {
    return this.links.receiveTripletNotification(options);
  }

  onTripletNotification(listener) {
    return this.links.onTripletNotification(listener);
  }

  close() {
    return this.links.close();
  }

  async [Symbol.asyncDispose]() {
    await this.close();
  }
}
