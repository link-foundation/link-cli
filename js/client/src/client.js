// The links client: one stable contract over interchangeable transports, the
// JS counterpart of `INamedTypesLinks` / `NamedTypeLinks` behind which the C#
// and Rust `RemoteLinks` swap a local store for a server.

import { GUARANTEES } from './capabilities.js';
import { scopeError, scopeOf } from './deadline.js';
import { LinksError, MalformedMessageError, RemoteError, UnsupportedError } from './errors.js';
import { formatDocument, parseDocument } from './lino.js';
import { errorMessage, replies, requests } from './operations.js';

/**
 * A links store behind a transport. Every address it takes or returns is a
 * BigInt (safe-integer numbers are accepted on input). Every method takes
 * `{ signal, deadline, timeout }`; a failed call rejects with a
 * {@link LinksError} whose `outcome` says whether the request may have run.
 *
 * A transport is `{ capabilities, send(text, { signal, deadline }), close() }`
 * where `send` resolves to the LiNo text of the reply.
 */
export class LinksClient {
  constructor(transport, { capabilities } = {}) {
    if (!transport || typeof transport.send !== 'function') throw new TypeError('a transport has a send(text, options) method');
    this.transport = transport;
    this.capabilities = Object.freeze({ ...transport.capabilities, ...capabilities });
  }

  /** Throws an UnsupportedError unless the backend offers every guarantee named. */
  require(...guarantees) {
    const missing = guarantees.filter((guarantee) => {
      if (!GUARANTEES.includes(guarantee)) throw new TypeError(`unknown guarantee "${guarantee}"; known: ${GUARANTEES.join(', ')}`);
      return this.capabilities[guarantee] !== true;
    });
    if (missing.length > 0) {
      throw new UnsupportedError(`the ${this.capabilities.backend} backend does not offer ${missing.join(', ')}; nothing is emulated`);
    }
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
    return replies.namedChanges(await this.request(parseDocument(text), options));
  }

  /** Every link, names substituted for named references (the empty request). */
  async exportLinks(options) {
    return replies.namedLinks(await this.request([], options));
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
   * `{ op: 'each', restriction }`, `{ op: 'query', text }`.
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
    };
    const build = documentOf[operation?.op];
    if (!build) throw new TypeError(`operation ${position}: unknown op "${operation?.op}"`);
    let request;
    let parse;
    try {
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

  close() {
    return this.links.close();
  }

  async [Symbol.asyncDispose]() {
    await this.close();
  }
}
