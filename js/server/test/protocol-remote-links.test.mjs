// A links store behind a links server (src/protocol-remote-links.js): ports
// rust/tests/remote_links_tests.rs, with its replay of
// docs/protocol/links-operations.txt, and the `RemoteLinks` cases of
// rust/tests/triplets_protocol_tests.rs.
//
// `RemoteLinks` is asynchronous in JavaScript, so it cannot be handed to the
// synchronous `QueryProcessor` or `processTripletsQuery`. Where Rust runs the
// query processor over a remote store, these tests send the same queries to
// the server and compare its replies with the changes the processor reports
// over a local store. The `doublets` crate traits (`Links`, `Doublets`) have no
// JavaScript counterpart, so the Rust tests of the raw links interface and of
// its restriction shapes (`matches`, covered by
// protocol-links-operations.test.mjs) have nothing to port here. Nor has the
// triplets case of the query processor over a remote store: Rust runs
// `process_triplets_query` in the client over `RemoteLinks`, and a server in
// triplet mode reads numbers, not names (triplets-substitution.test.mjs
// covers the processor itself).
import { after, describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { simplifyChanges } from '../src/changes-simplifier.js';
import { link, NULL_LINK } from '../src/link.js';
import { NamedTypesDecorator } from '../src/named-types.js';
import { ArityRange } from '../src/protocol-binary-packet.js';
import { BinaryLinoOptions } from '../src/protocol-binary.js';
import { listen } from '../src/ipc.js';
import { LinksClient } from '../src/protocol-client.js';
import { ProtocolError } from '../src/protocol-errors.js';
import { LinksOperation } from '../src/protocol-links-operations.js';
import { formatDocument, parseDocument } from '../src/protocol-lino.js';
import { BinaryLinoProtocol, LinoConnection, TextLinoProtocol } from '../src/protocol-protocols.js';
import { RemoteLinks } from '../src/protocol-remote-links.js';
import { LinksServer, ServerOptions } from '../src/protocol-server.js';
import { QueryProcessor } from '../src/query-processor.js';
import {
  DoubletRole,
  triplet,
  tripletChange,
  TripletQuery,
  TripletsOverDoublets,
} from '../src/triplets.js';
import { scratchDirectory } from './helpers/storage.mjs';

const PUSH_TIMEOUT = 10_000;
const SILENCE_TIMEOUT = 200;
const TIMEOUT = { timeout: 60_000 };

/** Every protocol the Rust tests cover: text, and binary with each option combination. */
function protocols() {
  const all = [new TextLinoProtocol()];
  for (const externalReferences of [false, true]) {
    for (const arity of [ArityRange.DOUBLETS, ArityRange.between(2, 3), ArityRange.atLeast(1)]) {
      for (const packedWidths of [false, true]) {
        const options = new BinaryLinoOptions({ externalReferences, arity, packedWidths });
        all.push(new BinaryLinoProtocol({ options }));
      }
    }
  }
  return all;
}

let databases = 0;

/** A fresh named store in a scratch directory. */
function localStore() {
  databases += 1;
  return NamedTypesDecorator.open(join(scratchDirectory(), `store-${databases}.links`));
}

/** A `RunningServer`: serves a fresh named store on 127.0.0.1:0 until `stop`. */
async function startServer(options = {}, store = localStore()) {
  const server = await LinksServer.listen('127.0.0.1:0', new ServerOptions(options));
  const address = `127.0.0.1:${server.localAddr().port}`;
  const serving = server.serve(store);
  const clients = [];
  let stopped = false;
  const running = {
    address,
    async client(protocol = new TextLinoProtocol()) {
      const client = await LinksClient.connect(address, protocol);
      clients.push(client);
      return client;
    },
    async remote(protocol = new TextLinoProtocol()) {
      return new RemoteLinks(await running.client(protocol));
    },
    async stop() {
      if (stopped) return;
      stopped = true;
      server.shutdownHandle().shutdown();
      await serving;
      server.disconnectAll();
      for (const client of clients) client.stream().shutdown();
    },
  };
  after(() => running.stop());
  return running;
}

/**
 * A server that answers every request with the documents of `reply`, as the
 * Rust `scripted_server` does; it hangs up when the client does.
 */
async function scriptedServer(reply) {
  const listener = await listen('127.0.0.1:0');
  const address = `127.0.0.1:${listener.localAddr().port}`;
  const answering = (async () => {
    const stream = await listener.accept();
    listener.close();
    const connection = new LinoConnection(stream, new TextLinoProtocol());
    try {
      while ((await connection.receive()) !== null) await connection.send(parseDocument(reply));
    } catch {
      // The client hung up.
    }
    stream.shutdown();
  })();
  const remote = await RemoteLinks.connect(address, new TextLinoProtocol());
  after(async () => {
    remote.client().stream().shutdown();
    await answering;
  });
  return remote;
}

/** The README walkthrough plus the cases that cascade. */
const QUERIES = [
  '() ((1 1))',
  '() ((2 2))',
  '((1: 1 1)) ((1: 1 2))',
  '((1: 1 2)) ((1: 2 1))',
  '((($i: $s $t)) (($i: $t $s)))',
  '((2: 2 2)) ((2: 1 1))',
  '() ((father: father father))',
  '() ((mother: mother mother))',
  '() ((child: father mother))',
  '((child: father mother)) ((child: father mother))',
  '((child: father mother)) ((child: mother father))',
  '((father: father father)) ()',
  '((1 *)) ()',
];

/** Every link of `store` with its name. */
async function namedLinks(store) {
  const links = [...(await store.allLinks())].sort((a, b) => a.index - b.index);
  const named = [];
  for (const value of links) named.push([value, await store.getName(value.index)]);
  return named;
}

/**
 * Runs every `NamedTypeLinks` call once and checks the answers, so a remote
 * store is held to exactly what a local one returns. Every call is awaited,
 * which a local store's plain values allow.
 */
async function exercise(store) {
  const ignore = () => {};
  assert.equal(await store.create(0, 0), 1);
  assert.equal(await store.getOrCreate(1, 1), 2);
  assert.equal(await store.getOrCreate(1, 1), 2);
  assert.equal(await store.search(1, 1), 2);
  assert.equal(await store.search(2, 2), null);
  assert.deepEqual(await store.getLink(2), link(2, 1, 1));
  assert.equal(await store.getLink(9), null);
  assert.equal(await store.exists(1), true);
  assert.equal(await store.exists(9), false);

  assert.equal(await store.ensureCreated(5), 5);
  assert.equal(await store.ensureCreated(5), 5);
  assert.equal(await store.ensureCreated(0), 0);
  assert.equal((await store.exists(3)) || (await store.exists(4)), false);

  assert.deepEqual(await store.updateObserved(5, 2, 2, ignore), link(5, 0, 0));
  await assert.rejects(async () => store.updateObserved(9, 1, 1, ignore));

  const changes = [];
  const merged = await store.updateObserved(5, 1, 1, (before, after) =>
    changes.push([before, after]),
  );
  assert.deepEqual(merged, link(5, 2, 2));
  // A server replies with the net change of every link it touched.
  assert.deepEqual(simplifyChanges(changes), [[link(5, 2, 2), NULL_LINK]]);
  assert.equal(await store.exists(5), false);

  await store.setName(2, 'pair');
  assert.equal(await store.getName(2), 'pair');
  assert.equal(await store.getByName('pair'), 2);
  assert.equal(await store.getByName('none'), null);
  await store.removeName(2);
  assert.equal(await store.getName(2), null);

  const deleted = [];
  const removed = await store.deleteObserved(1, (before, after) => deleted.push([before, after]));
  assert.deepEqual(removed, link(1, 0, 0));
  const net = simplifyChanges(deleted);
  assert.ok(
    net.some(([b, a]) => b.index === 1 && a.index === 0),
    'deletes 1',
  );
  assert.deepEqual(
    net.find(([b]) => b.index === 2),
    [link(2, 1, 1), NULL_LINK],
  );
  await assert.rejects(async () => store.deleteObserved(1, ignore));
  await store.save();
}

describe('remote links', TIMEOUT, () => {
  it('the query processor gives the same results on a remote store', async () => {
    const local = localStore();
    const processor = new QueryProcessor(false).withAutoCreateMissingReferences(true);
    // A server names every reference the store names after the query, so the
    // processor's changes are rendered that way to compare them.
    const expected = QUERIES.map((query) => {
      const changes = processor.processQuery(local, query);
      const name = (id) => local.getName(id) ?? String(id);
      const side = (value) =>
        value === null
          ? '()'
          : `((${name(value.index)}: ${name(value.source)} ${name(value.target)}))`;
      return changes.map(([before, after]) => `${side(before)} ${side(after)}`).join('\n');
    });
    const expectedLinks = await namedLinks(local);

    for (const protocol of protocols()) {
      const server = await startServer({ autoCreateMissingReferences: true });
      const remote = await server.remote(protocol);
      const answers = [];
      for (const query of QUERIES) answers.push(await remote.client().queryText(query));
      assert.deepEqual(answers, expected);
      assert.deepEqual(await namedLinks(remote), expectedLinks);
      await server.stop();
    }
  });

  it('every NamedTypeLinks call matches a local store', async () => {
    for (const protocol of protocols()) {
      const local = localStore();
      const server = await startServer();
      const remote = await server.remote(protocol);
      await exercise(local);
      await exercise(remote);
      assert.deepEqual(await remote.allLinks(), local.allLinks());
      assert.deepEqual(await namedLinks(remote), await namedLinks(local));
      await server.stop();
    }
  });

  it('reads a single link and every link of a restriction', async () => {
    const server = await startServer();
    const remote = await RemoteLinks.connectEndpoint(server.address, new TextLinoProtocol());
    assert.equal(remote.client().kind(), 'tcp');
    assert.equal(await remote.create(1, 1), 1);
    assert.equal(await remote.create(2, 2), 2);
    assert.deepEqual(await remote.link(2), link(2, 2, 2));
    assert.equal(await remote.link(3), null);
    assert.deepEqual(await remote.linksMatching([null, 1]), [link(1, 1, 1)]);
    assert.equal(await remote.countMatching([]), 2);
    assert.equal(formatDocument(await remote.execute(LinksOperation.count([2]))), '(count: 1)');
    remote.client().stream().shutdown();
    await server.stop();
  });

  it('server failures are errors, not panics', async () => {
    const server = await startServer();
    const remote = await RemoteLinks.connect(server.address, new TextLinoProtocol());
    await assert.rejects(
      remote.updateObserved(7, 1, 1, () => {}),
      (error) => error instanceof ProtocolError && error.message.endsWith('Link not found: 7'),
    );
    await assert.rejects(remote.deleteObserved(7), /Link not found: 7/);
    await assert.rejects(remote.countMatching([1, 2, 3, 4]), ProtocolError);
    remote.client().stream().shutdown();
    await server.stop();
  });

  it('a lost connection fails where the interface cannot', async () => {
    const server = await startServer({ triplets: true });
    const remote = await server.remote();
    await server.stop();

    await assert.rejects(remote.countMatching([]), ProtocolError);
    const lost = (error) => {
      assert.match(error.message, /^remote links store failed: /);
      assert.ok(error.cause instanceof ProtocolError);
      return true;
    };
    for (const call of [
      () => remote.create(1, 1),
      () => remote.ensureCreated(5),
      () => remote.getLink(1),
      () => remote.exists(1),
      () => remote.allLinks(),
      () => remote.search(1, 1),
      () => remote.getOrCreate(1, 1),
      () => remote.tripletMarkers(),
      () => remote.roleOf(1),
      () => remote.eachTriplet(TripletQuery.any()),
      () => remote.countTriplets(TripletQuery.any()),
      () => remote.doubletUsages(1),
    ]) {
      await assert.rejects(call(), lost);
    }
  });

  it('replies that miss the requested change are errors', async () => {
    // An empty reply is a valid document that reports no change at all.
    const remote = await scriptedServer('');
    await assert.rejects(remote.updateObserved(1, 1, 1), {
      message: 'malformed message: the reply holds no change of 1',
    });
    await assert.rejects(
      remote.deleteObserved(2, () => {}),
      {
        message: 'malformed message: the reply holds no change of 2',
      },
    );
    await assert.rejects(remote.setName(1, 'x'), { message: 'the set-name reply holds no link' });
    assert.equal(await remote.getByName('x'), null);
    assert.equal(await remote.getName(1), null);
    await assert.rejects(remote.create(1, 1), (error) => {
      assert.match(error.message, /^remote links store failed: /);
      assert.ok(error.message.endsWith('the create reply holds no created link'), error.message);
      return true;
    });
    await remote.removeName(1);
    await remote.unsubscribe(1);
    await remote.save();
  });

  it('misshapen name replies are errors', async () => {
    const remote = await scriptedServer('(count: 1)');
    await assert.rejects(remote.getName(1), {
      message: 'malformed message: expected a name reply, found (count: 1)',
    });
    await assert.rejects(remote.getByName('x'), {
      message: 'malformed message: expected a link reply, found (count: 1)',
    });
  });

  it('a set name answers where the names store keeps it', async () => {
    const server = await startServer();
    const remote = await server.remote();
    await remote.create(1, 1);
    assert.equal(typeof (await remote.setName(1, 'one')), 'number');
    assert.equal(await remote.getByName('one'), 1);
    await server.stop();
  });

  it('a subscriber is told about the links another connection changes', async () => {
    for (const protocol of [new TextLinoProtocol(), protocols()[4]]) {
      const server = await startServer();
      const subscriber = await server.remote(protocol);
      const writer = await server.remote();
      const subscription = await subscriber.subscribe([]);
      assert.equal(subscription, 1);
      await writer.create(0, 0);

      assert.deepEqual(await subscriber.receiveNotification(PUSH_TIMEOUT), {
        subscription,
        changes: [[NULL_LINK, link(1, 0, 0)]],
      });
      assert.equal(await subscriber.receiveNotification(SILENCE_TIMEOUT), null);

      await subscriber.unsubscribe(subscription);
      await writer.create(0, 0);
      assert.equal(await subscriber.receiveNotification(SILENCE_TIMEOUT), null);
      await server.stop();
    }
  });
});

/** The requests and replies of docs/protocol/links-operations.txt. */
function conversation() {
  const text = readFileSync(
    new URL('../../../docs/protocol/links-operations.txt', import.meta.url),
    'utf8',
  );
  const lines = text.split('\n').filter((line) => line !== '' && !line.startsWith('#'));
  const exchanges = [];
  let start = 0;
  while (start < lines.length) {
    let end = start + 1;
    while (end < lines.length && !lines[end].startsWith('> ')) end += 1;
    exchanges.push([lines[start].slice(2), lines.slice(start + 1, end).join('\n')]);
    start = end;
  }
  return exchanges;
}

describe('the shared conversation', TIMEOUT, () => {
  it('servers answer it over every protocol', async () => {
    const exchanges = conversation();
    assert.equal(exchanges.length, 19);
    for (const protocol of protocols()) {
      const server = await startServer();
      const client = await server.client(protocol);
      for (const [request, reply] of exchanges) {
        if (reply.startsWith('! ')) {
          await assert.rejects(client.queryText(request), (error) => {
            assert.ok(error.message.endsWith(reply.slice(2)), `${request}: ${error.message}`);
            return true;
          });
        } else {
          assert.deepEqual([request, await client.queryText(request)], [request, reply]);
        }
      }
      await server.stop();
    }
  });
});

/** Triplets over a fresh local named store, as `local_triplets` builds them. */
const localTriplets = () => TripletsOverDoublets.withNamedMarkers(localStore());

/** What `get_triplet` answers, through `roleOf`. */
async function getTriplet(store, index) {
  const role = await store.roleOf(index);
  return role?.kind === 'Triplet' ? role.triplet : null;
}

/** Every `Triplets` call once, returning what the store answered. */
async function exerciseTriplets(store) {
  const person = await store.createTripletPoint();
  const knows = await store.createTripletPoint();
  const alice = await store.createTriplet(person, 0, 0);
  const bob = await store.createTriplet(person, 0, 0);
  const first = await store.createTriplet(knows, alice, bob);
  const second = await store.createTriplet(knows, alice, bob);
  await store.createTripletPointAt(77);
  const { pair } = await store.tripletMarkers();
  const answers = [
    `${person} ${knows} ${alice} ${bob} ${first} ${second}`,
    await getTriplet(store, first),
    await getTriplet(store, pair),
    await getTriplet(store, 77),
    await store.countTriplets(TripletQuery.any()),
    await store.eachTriplet(TripletQuery.byType(knows)),
    await store.eachTriplet(TripletQuery.byFrom(alice)),
    await store.eachTriplet(TripletQuery.byTo(bob)),
    await store.updateTriplet(first, knows, bob, alice),
    await getTriplet(store, first),
    await store.deleteTriplet(alice),
    await store.eachTriplet(TripletQuery.any()),
  ];
  for (const call of [
    () => store.createTriplet(99, 0, 0),
    () => store.createTriplet(0, person, person),
    () => store.updateTriplet(99, knows, 0, 0),
    () => store.deleteTriplet(99),
  ]) {
    try {
      await call();
      answers.push('no error');
    } catch (error) {
      answers.push(error.message);
    }
  }
  return answers;
}

describe('remote triplets', TIMEOUT, () => {
  it('every triplets call matches a local store', async () => {
    const expected = await exerciseTriplets(localTriplets());
    for (const protocol of protocols()) {
      const server = await startServer({ triplets: true });
      const answers = await exerciseTriplets(await server.remote(protocol));
      assert.equal(answers.length, expected.length);
      answers.forEach((answer, position) => {
        const wanted = expected[position];
        if (typeof wanted === 'string' && answer !== wanted) {
          assert.ok(answer.endsWith(wanted), `${answer} != ${wanted}`);
        } else {
          assert.deepEqual(answer, wanted, `answer ${position}`);
        }
      });
      await server.stop();
    }
  });

  it('names stay available through remote links', async () => {
    const server = await startServer({ triplets: true });
    const remote = await server.remote();
    const point = await remote.createTripletPoint();
    await remote.setName(point, 'person');
    assert.equal(await remote.getByName('person'), point);
    assert.equal(await remote.getName(point), 'person');
    assert.equal(await remote.exists(point), true);
    assert.equal(await remote.exists(999), false);
    assert.equal((await remote.tripletMarkers()).pair, 1);
    assert.equal((await remote.tripletMarkers()).nothing, 2);
    await server.stop();
  });

  it('a remote store knows markers and triplets but not their doublets', async () => {
    const server = await startServer({ triplets: true });
    const remote = await server.remote();
    const person = await remote.createTripletPoint();
    const alice = await remote.createTriplet(person, 0, 0);
    const knows = await remote.createTriplet(person, alice, alice);
    // The address before `knows` is one of its wrappers or pairs.
    const doublet = knows - 1;

    assert.equal(await remote.roleOf(0), null);
    assert.equal(await remote.roleOf(1), DoubletRole.Marker);
    assert.equal(await remote.roleOf(2), DoubletRole.Marker);
    assert.equal((await remote.roleOf(person)).kind, 'Triplet');
    assert.equal(await remote.roleOf(doublet), null);
    assert.equal(await remote.roleOf(999), null);

    // How many other triplets use it, each counted once.
    assert.equal(await remote.doubletUsages(person), 2);
    assert.equal(await remote.doubletUsages(alice), 1);
    assert.equal(await remote.doubletUsages(knows), 0);
    await server.stop();
  });

  it("a remote subscriber is told about another connection's triplets", TIMEOUT, async () => {
    const all = protocols();
    for (const [first, second] of all.map((protocol, position) => [
      protocol,
      protocols()[position],
    ])) {
      const server = await startServer({ triplets: true });
      const subscriber = await server.remote(first);
      const writer = await server.remote(second);
      const person = await writer.createTripletPoint();
      const knows = await writer.createTripletPoint();

      const subscription = await subscriber.subscribeTriplets([null, person]);
      assert.equal(subscription, 1);
      const alice = await writer.createTriplet(person, 0, 0);
      // A triplet of another type is not pushed.
      await writer.createTriplet(knows, alice, alice);

      assert.deepEqual(await subscriber.receiveTripletNotification(PUSH_TIMEOUT), {
        subscription,
        changes: [tripletChange(null, triplet(alice, person, 0, 0))],
      });
      assert.equal(await subscriber.receiveTripletNotification(SILENCE_TIMEOUT), null);

      await subscriber.unsubscribe(subscription);
      await writer.createTriplet(person, 0, 0);
      assert.equal(await subscriber.receiveTripletNotification(SILENCE_TIMEOUT), null);
      await server.stop();
    }
  });

  it('triplet mode keeps an existing store of doublets readable', async () => {
    // The server only adds the markers; a store that had triplets keeps them.
    const store = localStore();
    const triplets = TripletsOverDoublets.withNamedMarkers(store);
    const point = triplets.createTripletPoint();
    const index = triplets.createTriplet(point, 0, 0);
    triplets.save();
    const server = await startServer({ triplets: true }, store);
    const remote = await server.remote();
    assert.ok(
      (await remote.tripletsMatching([index])).some(
        (value) => value.index === index && value.typeId === point,
      ),
    );
    assert.equal(await remote.countTripletsMatching([index]), 1);
    await server.stop();
  });

  it('replies that miss the requested triplet are errors', async () => {
    const remote = await scriptedServer('');
    await assert.rejects(remote.createTriplet(1, 0, 0), {
      message: 'malformed message: the create reply holds no created triplet',
    });
    await assert.rejects(remote.createTripletPoint(), {
      message: 'malformed message: the create reply holds no created triplet',
    });
    await assert.rejects(remote.updateTriplet(5, 1, 0, 0), {
      message: 'the reply holds no change of 5',
    });
    assert.deepEqual(await remote.deleteTriplet(5), []);
    await remote.createTripletPointAt(9);
    // No markers are named: neither is a marker then.
    assert.deepEqual({ ...(await remote.tripletMarkers()) }, { pair: 0, nothing: 0 });
    assert.equal(await remote.roleOf(1), null);
  });
});
