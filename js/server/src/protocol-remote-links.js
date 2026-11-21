// A links store that lives behind a links server: ports
// rust/src/protocol/remote_links.rs (`RemoteLinks`) and
// rust/src/protocol/remote_triplets.rs (its triplet interface over a server
// in triplet mode).
//
// Every call is one `LinksOperation` (or `TripletsOperation`) round trip. An
// observer of a write sees the net change of every link it touched.
//
// Rust's `RemoteLinks` is a synchronous store, usable wherever a local one is.
// A JavaScript socket cannot be read synchronously, so here every method
// returns a Promise: the methods keep the names of the store interfaces
// (`create`, `exists`, `updateObserved`, `eachTriplet`, …) but cannot be
// handed to the synchronous `QueryProcessor`. The `doublets` crate traits
// (`Links`, `Doublets`) have no JavaScript counterpart and are not ported.
// Where Rust panics with `remote links store failed: {error}` (the methods of
// interfaces that cannot fail), this throws an `Error` with that message.
import { isNull } from './link.js';
import { LinkError } from './errors.js';
import { LinksClient } from './protocol-client.js';
import { ProtocolError } from './protocol-errors.js';
import {
  LinksOperation,
  parseChanges,
  parseCount,
  parseLinkReply,
  parseLinks,
  parseName,
  parseNotification,
  parseSubscription,
} from './protocol-links-operations.js';
import { parseDocument } from './protocol-lino.js';
import {
  TripletsOperation,
  parseTripletChanges,
  parseTripletNotification,
  parseTriplets,
  restrictionOfQuery,
} from './protocol-triplets-operations.js';
import {
  DoubletRole,
  NOTHING_MARKER_NAME,
  PAIR_MARKER_NAME,
  TripletMarkers,
  TripletQuery,
} from './triplets.js';

/** `connected`: the failure of a call whose interface has no way to report one. */
async function connected(promise) {
  try {
    return await promise;
  } catch (error) {
    throw new Error(`remote links store failed: ${error.message}`, { cause: error });
  }
}

/** A links store served by a `LinksServer`. */
export class RemoteLinks {
  #client;
  #markers = null;

  /** Uses an existing connection. */
  constructor(client) {
    this.#client = client;
  }

  /** Connects to the TCP `address`. */
  static async connect(address, protocol) {
    return new RemoteLinks(await LinksClient.connect(address, protocol));
  }

  /** Connects to an IPC endpoint such as `unix:/tmp/links.sock`. */
  static async connectEndpoint(endpoint, protocol) {
    return new RemoteLinks(await LinksClient.connectEndpoint(endpoint, protocol));
  }

  client() {
    return this.#client;
  }

  /** Runs one operation on the server and returns its reply. */
  execute(operation) {
    return this.#client.request(operation.toDocument());
  }

  /** Subscribes to the changes of the links matching `restriction`; returns its number. */
  async subscribe(restriction) {
    return parseSubscription(await this.execute(LinksOperation.subscribe(restriction)));
  }

  /** Stops a subscription; an unknown number is ignored. */
  async unsubscribe(subscription) {
    await this.execute(LinksOperation.unsubscribe(subscription));
  }

  /** The next change pushed to this connection, waiting up to `timeoutMs`; null when none. */
  async receiveNotification(timeoutMs) {
    const document = await this.#client.receiveNotification(timeoutMs);
    return document === null ? null : parseNotification(document);
  }

  /** Number of links matching `restriction`. */
  async countMatching(restriction) {
    return parseCount(await this.execute(LinksOperation.count(restriction)));
  }

  /** Links matching `restriction`, ordered by index. */
  async linksMatching(restriction) {
    return parseLinks(await this.execute(LinksOperation.each(restriction)));
  }

  /** Runs a `create`, `update` or `delete` and returns its changes. */
  async changes(operation) {
    return parseChanges(await this.execute(operation));
  }

  /** The link stored at `index`, or null. */
  async link(index) {
    return (await this.linksMatching([index])).pop() ?? null;
  }

  async #createRemote(source, target) {
    const changes = await this.changes(LinksOperation.create(source, target));
    const found = changes.find(([before, after]) => isNull(before) && !isNull(after));
    if (found === undefined) {
      throw ProtocolError.malformed('the create reply holds no created link');
    }
    return found[1];
  }

  /** Runs an `update` or `delete` of `index`, reports its changes and returns `index` before. */
  async #observed(operation, index, observer) {
    const changes = await this.changes(operation);
    for (const [before, after] of changes) observer?.(before, after);
    const found = changes.find(([before]) => before.index === index);
    if (found === undefined) throw ProtocolError.malformed(`the reply holds no change of ${index}`);
    return found[0];
  }

  // The `NamedTypeLinks` interface.

  async create(source, target) {
    return (await connected(this.#createRemote(source, target))).index;
  }

  /** Asks for links until the server hands out `id`, then deletes the ones passed over. */
  async ensureCreated(id) {
    if (id === 0 || (await this.exists(id))) return id;
    const passedOver = [];
    for (;;) {
      const created = await this.create(0, 0);
      if (created === id) break;
      passedOver.push(created);
    }
    for (const address of passedOver) {
      await connected(this.changes(LinksOperation.delete(address)));
    }
    return id;
  }

  getLink(id) {
    return connected(this.link(id));
  }

  /** Counts, so it also holds on a server in triplet mode. */
  async exists(id) {
    return (await connected(this.countMatching([id]))) > 0;
  }

  updateObserved(id, source, target, observer) {
    return this.#observed(LinksOperation.update(id, source, target), id, observer);
  }

  deleteObserved(id, observer) {
    return this.#observed(LinksOperation.delete(id), id, observer);
  }

  allLinks() {
    return connected(this.linksMatching([]));
  }

  async search(source, target) {
    const found = await connected(this.linksMatching([null, source, target]));
    return found.length > 0 ? found[0].index : null;
  }

  async getOrCreate(source, target) {
    return (await this.search(source, target)) ?? this.create(source, target);
  }

  async getName(id) {
    return parseName(await this.execute(LinksOperation.getName(id)));
  }

  async setName(id, name) {
    const found = parseLinkReply(await this.execute(LinksOperation.setName(id, name)));
    if (found === null) throw new LinkError('the set-name reply holds no link');
    return found;
  }

  async getByName(name) {
    return parseLinkReply(await this.execute(LinksOperation.getByName(name)));
  }

  async removeName(id) {
    await this.execute(LinksOperation.removeName(id));
  }

  /** The server saves after every change, so there is nothing to flush. */
  async save() {}

  // The triplets of a server in triplet mode (remote_triplets.rs).

  executeTriplets(operation) {
    return this.#client.request(operation.toDocument());
  }

  async countTripletsMatching(restriction) {
    return parseCount(await this.executeTriplets(TripletsOperation.count(restriction)));
  }

  async tripletsMatching(restriction) {
    return parseTriplets(await this.executeTriplets(TripletsOperation.each(restriction)));
  }

  async tripletChanges(operation) {
    return parseTripletChanges(await this.executeTriplets(operation));
  }

  async subscribeTriplets(restriction) {
    return parseSubscription(await this.executeTriplets(TripletsOperation.subscribe(restriction)));
  }

  async receiveTripletNotification(timeoutMs) {
    const document = await this.#client.receiveNotification(timeoutMs);
    return document === null ? null : parseTripletNotification(document);
  }

  async #marker(name) {
    const reply = await connected(
      this.execute(LinksOperation.getByName(name)).then(parseLinkReply),
    );
    return reply ?? 0;
  }

  async #createdTriplet(operation) {
    const found = (await this.tripletChanges(operation)).find((change) => change.after !== null);
    if (found === undefined) {
      throw ProtocolError.malformed('the create reply holds no created triplet');
    }
    return found.after.index;
  }

  /** What the server names `Pair` and `Nothing`, asked once. */
  async tripletMarkers() {
    if (this.#markers === null) {
      const pair = await this.#marker(PAIR_MARKER_NAME);
      this.#markers = new TripletMarkers(pair, await this.#marker(NOTHING_MARKER_NAME));
    }
    return this.#markers;
  }

  async roleOf(id) {
    if (id === 0) return null;
    if ((await this.tripletMarkers()).contains(id)) return DoubletRole.Marker;
    const found = (await connected(this.tripletsMatching([id]))).pop();
    return found === undefined ? null : DoubletRole.Triplet(found);
  }

  createTriplet(typeId, from, to) {
    return this.#createdTriplet(TripletsOperation.create(typeId, from, to));
  }

  createTripletPoint() {
    return this.#createdTriplet(TripletsOperation.create(0, 0, 0));
  }

  /** `create` has no way to name an address, a substitution query does. */
  async createTripletPointAt(address) {
    // A query of numbers always parses: Rust's mapping of a parse error is moot.
    await this.#client.request(parseDocument(`() ((${address}: ${address} 0 0))`));
  }

  async updateTriplet(index, typeId, from, to) {
    const changes = await this.tripletChanges(TripletsOperation.update(index, typeId, from, to));
    const found = changes.find((change) => change.before !== null);
    if (found === undefined) throw new LinkError(`the reply holds no change of ${index}`);
    return found.before;
  }

  async deleteTriplet(index) {
    const changes = await this.tripletChanges(TripletsOperation.delete(index));
    return changes.filter((change) => change.before !== null).map((change) => change.before);
  }

  eachTriplet(query) {
    return connected(this.tripletsMatching(restrictionOfQuery(query)));
  }

  countTriplets(query) {
    return connected(this.countTripletsMatching(restrictionOfQuery(query)));
  }

  async doubletUsages(id) {
    const users = [
      ...(await this.eachTriplet(TripletQuery.byType(id))),
      ...(await this.eachTriplet(TripletQuery.byFrom(id))),
      ...(await this.eachTriplet(TripletQuery.byTo(id))),
    ];
    return new Set(users.filter((user) => user.index !== id).map((user) => user.index)).size;
  }
}
