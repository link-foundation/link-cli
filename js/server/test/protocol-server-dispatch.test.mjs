// The serving side of a links server (src/protocol-server-dispatch.js): ports
// rust/src/protocol/server/tests.rs, plus the job queue, the outbox, the
// subscriptions of a connection and the dispatcher, without the network.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { link, NULL_LINK } from '../src/link.js';
import { LinkStorage } from '../src/link-storage.js';
import { ArityRange } from '../src/protocol-binary-packet.js';
import { BinaryLinoOptions } from '../src/protocol-binary.js';
import { diagnostics } from '../src/protocol-errors.js';
import { formatDocument, parseDocument } from '../src/protocol-lino.js';
import { MessageFormat, ProtocolLimits } from '../src/protocol-protocols.js';
import { LinksSessions } from '../src/protocol-sessions.js';
import {
  Connection,
  DEFAULT_MAX_PENDING_NOTIFICATIONS,
  Dispatcher,
  JobQueue,
  Outbox,
  SHUTDOWN,
  Wakeup,
  admitted,
  closeJob,
  requestJob,
} from '../src/protocol-server-dispatch.js';
import { ServerOptions } from '../src/protocol-server.js';
import { QueryLimits } from '../src/query-options.js';
import { triplet, tripletChange } from '../src/triplets.js';
import { scratchDirectory } from './helpers/storage.mjs';

const TEXT = MessageFormat.TEXT;
/** Doublets and triplets: triplets are packed as `(index: (type from to))`. */
const PACKED = MessageFormat.binary(new BinaryLinoOptions({ arity: ArityRange.between(2, 3) }));

const outbox = () => new Outbox(DEFAULT_MAX_PENDING_NOTIFICATIONS, () => {});

/** Runs one text request of `connection`, as Rust's `request` helper does. */
function request(dispatcher, sessions, connection, box, text, format = TEXT) {
  const finished = [];
  const job = requestJob(connection, box, parseDocument(text), format, (ran) => finished.push(ran));
  assert.equal(dispatcher.run(sessions, job), true);
  assert.deepEqual(finished, [true]);
}

/** Takes the queue of `box`, as text. */
function queued(box) {
  const documents = box.queue.splice(0).map(([document]) => formatDocument(document));
  box.pendingReplies = 0;
  box.pendingNotifications = 0;
  return documents;
}

/** Runs `action` with the server trace collected; returns its lines. */
function traced(action) {
  const lines = [];
  const write = diagnostics.write;
  diagnostics.write = (stream, text) => lines.push(`${stream}: ${text}`);
  try {
    action();
  } finally {
    diagnostics.write = write;
  }
  return lines;
}

/** A writable stream that records what it is given. */
function sink() {
  return {
    written: [],
    flushes: 0,
    write(bytes) {
      this.written.push(Buffer.from(bytes).toString('latin1'));
    },
    flush() {
      this.flushes += 1;
    },
  };
}

/** Lets every pending promise callback run. */
const settle = () => new Promise((resolve) => setImmediate(resolve));

test('closing a connection drops its subscriptions', () => {
  const storage = new LinkStorage(join(scratchDirectory(), 'db.links'));
  const sessions = new LinksSessions(storage);
  const dispatcher = new Dispatcher(new ServerOptions(), null);
  const [subscriber, writer] = [outbox(), outbox()];

  request(dispatcher, sessions, 1, subscriber, '(subscribe: ())');
  request(dispatcher, sessions, 2, writer, '(create: (0 0))');
  assert.equal(dispatcher.connections.size, 2);
  assert.equal(sessions.sessions().length, 2);
  assert.deepEqual(queued(subscriber), ['(subscription: 1)', '(notify: 1)\n() ((1: 0 0))']);
  // A connection is never told about its own writes.
  assert.deepEqual(queued(writer), ['() ((1: 0 0))']);

  assert.equal(dispatcher.run(sessions, closeJob(1)), true);
  assert.deepEqual([...dispatcher.connections.keys()], [2]);
  // Its session is closed with it.
  assert.equal(sessions.sessions().length, 1);
  request(dispatcher, sessions, 2, writer, '(create: (1 1))');
  assert.deepEqual(queued(subscriber), []);
  assert.equal(dispatcher.run(sessions, SHUTDOWN), false);
  dispatcher.closeAll(sessions);
  assert.deepEqual(sessions.sessions(), []);
});

test('the jobs are plain records', () => {
  const done = () => {};
  assert.deepEqual(requestJob(3, 'outbox', [], TEXT, done), {
    kind: 'request',
    connection: 3,
    outbox: 'outbox',
    document: [],
    format: TEXT,
    done,
  });
  assert.deepEqual(closeJob(4), { kind: 'close', connection: 4 });
  assert.equal(SHUTDOWN.kind, 'shutdown');
  assert.equal(Object.isFrozen(SHUTDOWN), true);
  assert.equal(DEFAULT_MAX_PENDING_NOTIFICATIONS, 1024);
});

test('a wakeup wakes every waiter at once, and only those already waiting', async () => {
  const wakeup = new Wakeup();
  const woken = [];
  const first = wakeup.wait().then(() => woken.push('first'));
  const second = wakeup.wait().then(() => woken.push('second'));
  await settle();
  assert.deepEqual(woken, []);
  wakeup.notifyAll();
  await Promise.all([first, second]);
  assert.deepEqual(woken, ['first', 'second']);
  let late = false;
  wakeup.wait().then(() => {
    late = true;
  });
  await settle();
  assert.equal(late, false);
  wakeup.notifyAll();
  await settle();
  assert.equal(late, true);
});

test('the job queue hands out jobs in order and waits for the next one', async () => {
  const jobs = new JobQueue();
  assert.equal(jobs.send(closeJob(1)), true);
  assert.equal(jobs.send(closeJob(2)), true);
  assert.deepEqual(await jobs.recv(), closeJob(1));
  assert.deepEqual(await jobs.recv(), closeJob(2));
  const waiting = jobs.recv();
  await settle();
  jobs.send(SHUTDOWN);
  assert.equal(await waiting, SHUTDOWN);
  assert.equal(jobs.isClosed(), false);
});

test('closing the job queue tells every queued request it will not run', async () => {
  const jobs = new JobQueue();
  const told = [];
  jobs.send(requestJob(1, null, [], TEXT, (ran) => told.push(['first', ran])));
  jobs.send(closeJob(1));
  jobs.send(requestJob(2, null, [], TEXT, (ran) => told.push(['second', ran])));
  jobs.close();
  assert.deepEqual(told, [
    ['first', false],
    ['second', false],
  ]);
  assert.equal(jobs.isClosed(), true);
  // A closed queue ends the serving loop.
  assert.equal(await jobs.recv(), SHUTDOWN);
  assert.equal(jobs.send(SHUTDOWN), false);
  // Closing again changes nothing.
  jobs.close();
  assert.equal(told.length, 2);
});

test('closing the job queue wakes a waiting receiver with a shutdown', async () => {
  const jobs = new JobQueue();
  const waiting = jobs.recv();
  await settle();
  jobs.close();
  assert.equal(await waiting, SHUTDOWN);
});

test('an outbox counts replies and pushes until they are written', async () => {
  const box = new Outbox(2);
  box.reply(parseDocument('(count: 1)'), TEXT);
  assert.equal(box.push(parseDocument('(notify: 1)'), TEXT), true);
  assert.equal(box.push(parseDocument('(notify: 2)'), TEXT), true);
  assert.equal(box.pendingReplies, 1);
  assert.equal(box.pendingNotifications, 2);
  const stream = sink();
  const writing = box.writeAll(stream, new ProtocolLimits());
  await box.waitForReplies();
  await settle();
  assert.equal(box.pendingReplies, 0);
  assert.equal(box.pendingNotifications, 0);
  assert.deepEqual(stream.written, ['(count: 1)\n.\n', '(notify: 1)\n.\n', '(notify: 2)\n.\n']);
  // Nothing is waiting: the writer waits for more.
  box.reply(parseDocument('(x)'), PACKED);
  await box.waitForReplies();
  assert.equal(stream.written.length, 4);
  assert.equal(stream.written[3].charCodeAt(0) >> 4, 1);
  box.close();
  await writing;
  assert.equal(box.isClosed(), true);
});

test('an outbox with too many pushes waiting closes and hangs up once', () => {
  let hangUps = 0;
  const box = new Outbox(1, () => {
    hangUps += 1;
  });
  assert.equal(box.push(parseDocument('(notify: 1)'), TEXT), true);
  assert.equal(box.push(parseDocument('(notify: 2)'), TEXT), false);
  assert.equal(box.isClosed(), true);
  assert.equal(hangUps, 1);
  assert.deepEqual(box.queue, []);
  // A closed outbox takes nothing and hangs up no more.
  assert.equal(box.push(parseDocument('(notify: 3)'), TEXT), false);
  box.reply(parseDocument('(count: 1)'), TEXT);
  assert.deepEqual(box.queue, []);
  assert.equal(box.pendingReplies, 0);
  box.close();
  assert.equal(hangUps, 1);
});

test('the default outbox allows 1024 pushes and does nothing on hang up', () => {
  const box = new Outbox();
  assert.equal(box.maxPendingNotifications, 1024);
  box.close();
  assert.equal(box.isClosed(), true);
});

test('closing an outbox ends the wait for its replies', async () => {
  const box = outbox();
  box.reply(parseDocument('(count: 1)'), TEXT);
  const waiting = box.waitForReplies();
  await settle();
  box.close();
  await waiting;
  assert.equal(box.pendingReplies, 1);
});

test('an outbox that cannot write closes and passes the failure on', async () => {
  let hungUp = false;
  const box = new Outbox(DEFAULT_MAX_PENDING_NOTIFICATIONS, () => {
    hungUp = true;
  });
  box.reply(parseDocument('(count: 1)'), TEXT);
  const broken = {
    write() {
      throw new Error('broken pipe');
    },
    flush() {},
  };
  await assert.rejects(box.writeAll(broken, new ProtocolLimits()), {
    message: 'I/O error: broken pipe',
  });
  assert.equal(box.isClosed(), true);
  assert.equal(hungUp, true);
});

test('an outbox closed while a document is written stops after it', async () => {
  const box = outbox();
  box.reply(parseDocument('(count: 1)'), TEXT);
  box.reply(parseDocument('(count: 2)'), TEXT);
  const stream = sink();
  const write = stream.write.bind(stream);
  stream.write = (bytes) => {
    write(bytes);
    box.close();
  };
  await box.writeAll(stream, new ProtocolLimits());
  assert.deepEqual(stream.written, ['(count: 1)\n.\n']);
  assert.equal(box.pendingReplies, 2);
});

test('a connection answers subscribe and unsubscribe, and nothing else', () => {
  const connection = new Connection(outbox(), 0);
  const execute = (text) => {
    const reply = connection.execute(parseDocument(text), TEXT, false);
    return reply === null ? null : formatDocument(reply);
  };
  assert.equal(execute('(subscribe: ())'), '(subscription: 1)');
  assert.equal(execute('(subscribe: (* 1 *))'), '(subscription: 2)');
  assert.equal(execute('(unsubscribe: 1)'), '');
  // An unknown number is ignored.
  assert.equal(execute('(unsubscribe: 7)'), '');
  assert.deepEqual([...connection.subscriptions.keys()], [2]);
  assert.deepEqual(connection.subscriptions.get(2), [[null, 1, null], TEXT]);
  assert.equal(execute('(count: ())'), null);
  assert.equal(execute('() ((1 1))'), null);
  // A malformed operation is the sessions' to answer.
  assert.equal(execute('(subscribe: (1 2 3 4))'), null);
  assert.equal(execute('(unsubscribe: x)'), null);
  assert.equal(connection.session, 0);
});

test('a connection in triplet mode subscribes to triplets', () => {
  const connection = new Connection(outbox(), 0);
  const execute = (text) => {
    const reply = connection.execute(parseDocument(text), TEXT, true);
    return reply === null ? null : formatDocument(reply);
  };
  assert.equal(execute('(subscribe: (* 5 * *))'), '(subscription: 1)');
  assert.deepEqual(connection.subscriptions.get(1), [[null, 5, null, null], TEXT]);
  // Any other triplet operation, or a malformed one, is the sessions' to answer.
  assert.equal(execute('(count: ())'), null);
  assert.equal(execute('(subscribe: (1 2 3 4 5))'), null);
  // Unsubscribing is no triplet operation.
  assert.equal(execute('(unsubscribe: 1)'), '');
  assert.equal(connection.subscriptions.size, 0);
  assert.equal(execute('() ((1 1))'), null);
});

test('subscription numbers wrap around as a u32 does', () => {
  const connection = new Connection(outbox(), 0);
  connection.nextSubscription = 0xffffffff;
  assert.equal(formatDocument(connection.subscribe([], TEXT)), '(subscription: 4294967295)');
  assert.equal(formatDocument(connection.subscribe([], TEXT)), '(subscription: 0)');
  assert.equal(connection.nextSubscription, 1);
});

test('a connection pushes each change to every subscription it fits, in order', () => {
  const box = outbox();
  const connection = new Connection(box, 0);
  connection.nextSubscription = 5;
  connection.subscribe([null, 1, null], TEXT);
  connection.nextSubscription = 3;
  connection.subscribe([], TEXT);
  connection.subscribe([null, 7, 7], TEXT);
  const changes = [
    [NULL_LINK, link(1, 1, 1)],
    [link(2, 1, 2), link(2, 2, 2)],
    [link(3, 3, 3), NULL_LINK],
  ];
  connection.notify(changes, () => true);
  assert.deepEqual(queued(box), [
    '(notify: 3)\n() ((1: 1 1))\n((2: 1 2)) ((2: 2 2))\n((3: 3 3)) ()',
    '(notify: 5)\n() ((1: 1 1))\n((2: 1 2)) ()',
  ]);
  // What may not be read is left out, as is a change with nothing left.
  connection.notify(changes, (value) => value.index !== 1);
  assert.deepEqual(queued(box), [
    '(notify: 3)\n((2: 1 2)) ((2: 2 2))\n((3: 3 3)) ()',
    '(notify: 5)\n((2: 1 2)) ()',
  ]);
  connection.notify(changes, () => false);
  assert.deepEqual(queued(box), []);
});

test('a connection stops pushing once its outbox overflows', () => {
  let hangUps = 0;
  const box = new Outbox(1, () => {
    hangUps += 1;
  });
  const connection = new Connection(box, 0);
  connection.subscribe([], TEXT);
  connection.subscribe([], TEXT);
  connection.subscribe([], TEXT);
  connection.notify([[NULL_LINK, link(1, 0, 0)]], () => true);
  assert.equal(box.isClosed(), true);
  assert.equal(hangUps, 1);
});

test('a connection pushes the triplets its subscriptions fit, packed for its format', () => {
  const box = outbox();
  const connection = new Connection(box, 0);
  connection.subscribe([], PACKED);
  connection.subscribe([null, 5, null, null], TEXT);
  connection.notifyTriplets([], () => true);
  assert.deepEqual(box.queue, []);
  const changes = [
    tripletChange(null, triplet(10, 5, 1, 2)),
    tripletChange(triplet(11, 6, 1, 2), triplet(11, 5, 2, 1)),
    tripletChange(triplet(12, 6, 0, 0), null),
  ];
  const asked = [];
  connection.notifyTriplets(changes, (value) => {
    asked.push(value);
    return value.index !== 12;
  });
  assert.deepEqual(queued(box), [
    '(notify: 1)\n() ((10: (5 1 2)))\n((11: (6 1 2))) ((11: (5 2 1)))',
    '(notify: 2)\n() ((10: 5 1 2))\n() ((11: 5 2 1))',
  ]);
  // The outer doublet of a triplet is what may or may not be read.
  assert.deepEqual(asked.slice(0, 4), [
    link(10, 5, 10),
    link(11, 6, 11),
    link(11, 5, 11),
    link(12, 6, 12),
  ]);
  connection.notifyTriplets(changes, () => false);
  assert.deepEqual(queued(box), []);
});

test('a connection stops pushing triplets once its outbox overflows', () => {
  let hangUps = 0;
  const box = new Outbox(0, () => {
    hangUps += 1;
  });
  const connection = new Connection(box, 0);
  connection.subscribe([], TEXT);
  connection.subscribe([], TEXT);
  connection.notifyTriplets([tripletChange(null, triplet(1, 1, 0, 0))], () => true);
  assert.equal(box.isClosed(), true);
  assert.equal(hangUps, 1);
});

test('a tracing dispatcher traces every request and reply', () => {
  const sessions = new LinksSessions(LinkStorage.inMemory());
  const dispatcher = new Dispatcher(new ServerOptions({ trace: true }));
  const box = outbox();
  const lines = traced(() => {
    request(dispatcher, sessions, 1, box, '() ((1 1))');
    request(dispatcher, sessions, 1, box, '(subscribe: ())');
  });
  assert.deepEqual(lines, [
    'stderr: [server] request: () ((1 1))\n',
    'stderr: [server] reply: () ((1: 1 1))\n',
    'stderr: [server] request: (subscribe: ())\n',
    'stderr: [server] reply: (subscription: 1)\n',
  ]);
  assert.deepEqual(
    traced(() => dispatcher.run(sessions, closeJob(1))),
    [],
  );
});

test('a request the store does not admit is not traced, its reply is', () => {
  const storage = LinkStorage.inMemory();
  storage.queryLimits = () => tightLimits;
  const sessions = new LinksSessions(storage);
  const dispatcher = new Dispatcher(new ServerOptions({ trace: true }));
  const lines = traced(() => request(dispatcher, sessions, 1, outbox(), '(a b c d)'));
  assert.deepEqual(lines, ["stderr: [server] reply: (error: 'protocol node limit exceeded')\n"]);
});

test('closing an unknown connection changes nothing', () => {
  const sessions = new LinksSessions(LinkStorage.inMemory());
  const dispatcher = new Dispatcher(new ServerOptions());
  assert.equal(dispatcher.run(sessions, closeJob(9)), true);
  assert.equal(dispatcher.connections.size, 0);
  assert.deepEqual(sessions.sessions(), []);
});

test("a dispatcher's readable hides links from every subscriber", () => {
  const sessions = new LinksSessions(LinkStorage.inMemory());
  const dispatcher = new Dispatcher(new ServerOptions(), (value) => value.source !== 2);
  const [subscriber, writer] = [outbox(), outbox()];
  request(dispatcher, sessions, 1, subscriber, '(subscribe: ())');
  for (const text of ['(create: (0 0))', '(update: (1 2 2))', '(update: (1 2 1))']) {
    request(dispatcher, sessions, 2, writer, text);
  }
  // A read that changes nothing pushes nothing.
  request(dispatcher, sessions, 2, writer, '(count: ())');
  assert.deepEqual(queued(subscriber), [
    '(subscription: 1)',
    '(notify: 1)\n() ((1: 0 0))',
    '(notify: 1)\n((1: 0 0)) ()',
  ]);
});

test('a dispatcher in triplet mode pushes the triplets other connections change', () => {
  const sessions = new LinksSessions(LinkStorage.inMemory()).withTriplets();
  const dispatcher = new Dispatcher(new ServerOptions());
  const [subscriber, writer, other] = [outbox(), outbox(), outbox()];
  request(dispatcher, sessions, 1, subscriber, '(subscribe: ())', PACKED);
  request(dispatcher, sessions, 3, other, '(count: ())');
  request(dispatcher, sessions, 2, writer, '(create: (0 0 0))', PACKED);
  assert.deepEqual(queued(subscriber), ['(subscription: 1)', '(notify: 1)\n() ((3: (3 0 0)))']);
  assert.deepEqual(queued(writer), ['() ((3: (3 0 0)))']);
  assert.deepEqual(queued(other), ['(count: 0)']);
});

const tightLimits = new QueryLimits({
  maxAddress: 0xffffffff,
  maxBytes: 1024,
  maxDepth: 8,
  maxNodes: 2,
  maxResults: 16,
  maxBindings: 16,
  maxBindingBytes: 1024,
});

test('a store without limits admits every request', () => {
  const unlimited = { queryLimits: () => null };
  const limited = { queryLimits: () => tightLimits };
  assert.equal(admitted(unlimited, parseDocument('(a b c d)')), true);
  assert.equal(admitted(limited, parseDocument('(a)')), true);
  assert.equal(admitted(limited, parseDocument('(a b c d)')), false);
});
