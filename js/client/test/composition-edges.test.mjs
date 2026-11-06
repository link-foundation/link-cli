// The refusals and forwarding of the composing layers (issue #149) and of the
// subscription surface they share. Each case states an observable guarantee of
// those layers rather than repeating the scenarios of union-remap.test.mjs.

import assert from "node:assert/strict";
import { createServer } from "node:net";
import test from "node:test";
import {
  AddressRangeError,
  AddressTable,
  ANY,
  encodeMessage,
  ExternalIdMap,
  LinksClient,
  LinksDecorator,
  LinksError,
  LinksExternalIds,
  OffsetAddressMap,
  RemapLinks,
  tcpServerCapabilities,
  UnionLinks,
  UnsupportedError,
} from "../src/index.js";
import {
  byIndex,
  checkRestriction,
  isAny,
  isNullAddress,
} from "../src/composed.js";
import { ProcessTransport } from "../src/process.js";
import { connectTcp } from "../src/tcp.js";
import { MalformedMessageError } from "../src/errors.js";
import {
  parseNamedTriplet,
  parseTripletNotification,
  replies,
} from "../src/operations.js";
import { all, make, MemoryLinks } from "./memory-links.mjs";

function scripted(answer = () => "", capabilities = tcpServerCapabilities()) {
  const sent = [];
  return {
    sent,
    capabilities,
    async send(text) {
      sent.push(text);
      return answer(text);
    },
    async close() {},
  };
}

test("a query reaches a lossless peer verbatim after admission, and others as parsed", async () => {
  const lossless = scripted(() => "", {
    ...tcpServerCapabilities(),
    losslessQueries: true,
  });
  await new LinksClient(lossless).query("((1: 1 1)) ((1: 2 2))");
  assert.equal(lossless.sent.length, 1);
  assert.match(lossless.sent[0], /^\(query:/);

  const legacy = scripted();
  await new LinksClient(legacy).query("((1: 1 1)) ((1: 2 2))");
  assert.doesNotMatch(legacy.sent[0], /^\(query:/);
});

test("notification listeners are functions and a push-less transport has none pending", () => {
  const transport = scripted();
  transport.receivePush = async () => "";
  transport.onPush = () => () => {};
  const client = new LinksClient(transport);
  assert.throws(() => client.onNotification("listener"), TypeError);
  assert.equal(client.pendingNotifications, 0);
  transport.pendingPushes = 2;
  assert.equal(client.pendingNotifications, 2);
});

test("only a passed deadline reads as no triplet notification", async () => {
  const failure = new Error("the wire broke");
  const transport = scripted(() => "", {
    ...tcpServerCapabilities(),
    triplets: true,
  });
  transport.onPush = () => () => {};
  transport.receivePush = async () => {
    throw failure;
  };
  await assert.rejects(
    new LinksClient(transport).receiveTripletNotification(),
    failure,
  );
});

test("a decorator forwards the subscription surface", () => {
  const called = [];
  const links = {
    subscribe: (...args) => called.push(["subscribe", ...args]),
    unsubscribe: (...args) => called.push(["unsubscribe", ...args]),
    receiveNotification: (...args) =>
      called.push(["receiveNotification", ...args]),
    onNotification: (...args) => called.push(["onNotification", ...args]),
    pendingNotifications: 7,
  };
  const decorated = new LinksDecorator(links);
  decorated.subscribe([1], "o");
  decorated.unsubscribe(3, "o");
  decorated.receiveNotification("o");
  decorated.onNotification("l");
  assert.equal(decorated.pendingNotifications, 7);
  assert.deepEqual(called, [
    ["subscribe", [1], "o"],
    ["unsubscribe", 3, "o"],
    ["receiveNotification", "o"],
    ["onNotification", "l"],
  ]);
});

test("every kind of match-anything part reads as any, and addresses do not", () => {
  for (const part of [null, undefined, ANY]) assert.equal(isAny(part), true);
  assert.equal(isAny(0n), false);
});

test("a batch of a composed layer refuses what it cannot translate before running", async () => {
  const inner = new MemoryLinks();
  const remap = new RemapLinks(inner, new OffsetAddressMap(10n));
  for (const operations of [
    [{ op: "query", text: "()" }],
    [{ op: "countTriplets" }],
  ]) {
    await assert.rejects(remap.batch(operations), UnsupportedError);
  }
  await assert.rejects(remap.batch([{}]), TypeError);
  await assert.rejects(remap.batch([null]), TypeError);
  await assert.rejects(remap.batch("count"), TypeError);
  assert.equal(inner.links.size, 0);
});

test("a batch of a composed layer runs every translated operation", async () => {
  const union = new UnionLinks([new MemoryLinks()]);
  const { completed, results } = await union.batch([
    { op: "create", source: 1n, target: 1n },
    { op: "setName", index: 1n, name: "first" },
    { op: "getName", index: 1n },
    { op: "getByName", name: "first" },
    { op: "each" },
    { op: "count" },
    { op: "removeName", index: 1n },
    { op: "each", restriction: [1n] },
    { op: "count", restriction: [1n] },
  ]);
  assert.equal(completed, true);
  assert.equal(results[2].value, "first");
  assert.equal(results[3].value, 1n);
  assert.equal(results[4].value.length, 1);
  assert.equal(results[5].value, 1n);
  assert.equal(results[8].value, 1n);
});

test("a non-links failure in a composed batch has an unknown outcome and stops it", async () => {
  const inner = new MemoryLinks();
  inner.count = async () => {
    throw new Error("not a links error");
  };
  const { results, unknown, notSent } = await new UnionLinks([inner]).batch([
    { op: "getName", index: 1n },
    { op: "each", restriction: [ANY] },
  ]);
  assert.equal(results[0].status, "unknown");
  assert.equal(unknown, 1);
  assert.equal(notSent, 1);
});

test("a union rejects non-members and refuses raw text and subscriptions", async () => {
  for (const members of ["members", []])
    assert.throws(() => new UnionLinks(members), TypeError);
  for (const member of [null, {}])
    assert.throws(() => new UnionLinks([member]), TypeError);
  const union = new UnionLinks([new MemoryLinks()]);
  for (const refused of [
    union.request(),
    union.requestText(),
    union.subscribe(),
    union.unsubscribe(),
    union.receiveNotification(),
  ]) {
    await assert.rejects(refused, UnsupportedError);
  }
  assert.throws(() => union.onNotification(), UnsupportedError);
  assert.equal(union.pendingNotifications, 0);
});

test("a union propagates member failures that are not address ranges", async () => {
  const failure = new Error("the member broke");
  const broken = new MemoryLinks();
  broken.count = async () => {
    throw failure;
  };
  await assert.rejects(new UnionLinks([broken]).ownerOf(1n), failure);

  const unreadable = new MemoryLinks();
  unreadable.each = async () => {
    throw failure;
  };
  await assert.rejects(new UnionLinks([unreadable]).each([]), failure);

  const unnamed = new MemoryLinks();
  unnamed.count = async () => 0n;
  unnamed.getName = async () => {
    throw failure;
  };
  await assert.rejects(new UnionLinks([unnamed]).getName(1n), failure);
});

test("a union treats a range rejection as absence for ownership, scans and names", async () => {
  const outOfRange = new MemoryLinks("narrow");
  const refuse = async () => {
    throw new AddressRangeError("past this member");
  };
  outOfRange.count = refuse;
  outOfRange.each = refuse;
  outOfRange.getName = refuse;
  const wide = new MemoryLinks("wide");
  const index = await make(wide, 1n, 1n);
  await wide.setName(index, "kept");
  const union = new UnionLinks([outOfRange, wide]);
  assert.equal(await union.ownerOf(index), 1);
  assert.deepEqual(await all(union), [[index, 1n, 1n]]);

  const missing = new UnionLinks([outOfRange]);
  assert.equal(await missing.getName(index), null);
  const nameless = new UnionLinks([outOfRange, wide]);
  wide.count = async () => 0n;
  assert.equal(await nameless.getName(index), "kept");
  await wide.removeName(index);
  assert.equal(await nameless.getName(index), null);
});

test("a union reports the largest member range, whichever member holds it", () => {
  const widths = [50n, 500n, 5n];
  const members = widths.map((maxAddress) => {
    const member = new MemoryLinks();
    member.capabilities = { ...member.capabilities, maxAddress };
    return member;
  });
  assert.equal(new UnionLinks(members).capabilities.maxAddress, 500n);
});

test("a remap refuses raw text and rejects bad maps and listeners", async () => {
  for (const map of [null, {}, { toInner() {} }]) {
    assert.throws(() => new RemapLinks(new MemoryLinks(), map), TypeError);
  }
  const remap = new RemapLinks(new MemoryLinks(), new OffsetAddressMap(10n));
  assert.equal(remap.require(), remap);
  assert.throws(() => remap.require("atomicTransactions"), UnsupportedError);
  for (const refused of [
    remap.request(),
    remap.requestText(),
    remap.exportLinks(),
  ]) {
    await assert.rejects(refused, UnsupportedError);
  }
  assert.throws(() => remap.onNotification("listener"), TypeError);
});

test("a remap keeps a non-numeric maximum and passes an empty notification read", async () => {
  const inner = new MemoryLinks();
  inner.receiveNotification = async () => null;
  const remap = new RemapLinks(inner, new OffsetAddressMap(10n));
  assert.equal(await remap.receiveNotification(), null);
  assert.equal(new OffsetAddressMap(10n).maxAddress(null), null);
  assert.equal(new OffsetAddressMap(10n).maxAddress(5n), 15n);

  const opaque = { toInner: (outer) => outer, toOuter: (value) => value };
  assert.equal(
    new RemapLinks(inner, opaque).capabilities.maxAddress,
    inner.capabilities.maxAddress,
  );
});

test("a remap reports a listener failure without losing the next notification", async () => {
  const inner = new MemoryLinks();
  let deliver;
  inner.onNotification = (listener) => {
    deliver = listener;
    return () => {};
  };
  const failure = new Error("the listener broke");
  const remap = new RemapLinks(inner, new OffsetAddressMap(10n));
  const delivered = [];
  remap.onNotification((notification) => {
    delivered.push(notification.subscription);
    if (notification.subscription === 1) throw failure;
  });
  assert.equal(
    await uncaughtFrom(() => deliver({ subscription: 1, changes: [] })),
    failure,
  );
  deliver({ subscription: 2, changes: [] });
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(delivered, [1, 2]);
});

test("an address table refuses conflicting mappings and null, keeping pairs ordered", () => {
  const table = new AddressTable({ first: 100n });
  table.add(102n, 2n);
  table.add(101n, 1n);
  table.add(101n, 1n);
  table.add(103n, 3n);
  assert.deepEqual(table.pairs(), [
    [101n, 1n],
    [102n, 2n],
    [103n, 3n],
  ]);
  assert.throws(() => table.add(0n, 5n), AddressRangeError);
  assert.throws(() => table.add(105n, 0n), AddressRangeError);
  assert.throws(() => table.add(101n, 9n), AddressRangeError);
  assert.throws(() => table.add(109n, 1n), AddressRangeError);
  assert.throws(() => new AddressTable({ newId: "id" }), TypeError);
});

test("a store that merges a new link cannot host an external id", async () => {
  const store = new MemoryLinks();
  store.create = async () => null;
  await assert.rejects(
    new LinksExternalIds("s").bind(store, "id", 1n),
    (error) => {
      assert.ok(error instanceof LinksError);
      assert.equal(error.outcome, "failed");
      return true;
    },
  );
});

test("external ids read as absent on every malformed or missing table state", async () => {
  const ids = new LinksExternalIds("s");
  assert.throws(() => ids.nameOf(""), TypeError);
  assert.throws(() => ids.nameOf(5), TypeError);
  const store = new MemoryLinks();
  assert.equal(await ids.idOf(store, 1n), null);
  const target = await make(store, 1n, 1n);
  const pair = await ids.bind(store, "id", target);
  assert.equal(await ids.idOf(store, 0n), null);
  assert.equal(await ids.idOf(store, 999n), null);
  assert.equal(await ids.find(store, "absent"), null);
  assert.equal(await ids.unbind(store, "absent"), false);

  // The pair's name points elsewhere: an id belongs only to a pair of its scheme.
  await store.setName(pair, "other");
  assert.equal(await ids.idOf(store, target), null);
  await store.removeName(pair);
  assert.equal(await ids.idOf(store, target), null);

  // A name bound to a link outside the scheme does not read as the binding.
  const stranger = await make(store, target, target);
  await store.setName(stranger, "s:foreign");
  assert.equal(await ids.find(store, "foreign"), null);
});

test("external ids rebuild a root and tolerate a store answering zero for no name", async () => {
  const ids = new LinksExternalIds("zero");
  const store = new MemoryLinks();
  const getByName = store.getByName.bind(store);
  store.getByName = async (name, options) =>
    (await getByName(name, options)) ?? 0n;
  assert.equal(await ids.find(store, "none"), null);
  const target = await make(store, 1n, 1n);
  const pair = await ids.bind(store, "id", target);
  assert.equal(await ids.find(store, "id"), target);
  assert.equal(await ids.idOf(store, target), "id");
  assert.equal((await store.each([pair])).length, 1);

  // A dangling root is replaced and the scheme linked to it again.
  const dangling = new MemoryLinks();
  const danglingTarget = await make(dangling, 1n, 1n);
  await dangling.setName(999n, "ExternalId");
  await ids.bind(dangling, "id", danglingTarget);
  assert.equal(await ids.find(dangling, "id"), danglingTarget);
});

test("external ids survive a merging create and a link deleted under its pair", async () => {
  const ids = new LinksExternalIds("m");
  const store = new MemoryLinks();
  const target = await make(store, 1n, 1n);
  await ids.bind(store, "seed", target);

  const original = store.create.bind(store);
  store.create = async (...args) => {
    await original(...args);
    return null;
  };
  const other = 5n;
  const pair = await ids.bind(store, "merged", other);
  assert.equal((await store.each([pair]))[0].target, other);
  store.create = original;

  const getByName = store.getByName.bind(store);
  store.getByName = async (name, options) =>
    name === "m:gone" ? 999n : getByName(name, options);
  assert.equal(await ids.find(store, "gone"), null);
});

test("an external id map validates its parts and accepts a shared scheme", async () => {
  assert.throws(() => new ExternalIdMap(null, "s"), TypeError);
  assert.throws(() => new ExternalIdMap({}, "s"), TypeError);
  assert.throws(
    () => new ExternalIdMap(new MemoryLinks(), "s", { newId: "id" }),
    TypeError,
  );
  const scheme = new LinksExternalIds("shared");
  const map = new ExternalIdMap(new MemoryLinks(), scheme);
  assert.equal(map.ids, scheme);
  assert.equal(await map.toInner(""), null);
  assert.equal(await map.toInner(7), null);
  assert.equal(typeof (await map.toOuter(5n)), "string");
});

test("malformed named triplets and subscription replies are rejected", () => {
  assert.throws(
    () =>
      parseNamedTriplet({
        id: "1",
        values: [{ id: null, values: [] }, "2", "3"],
      }),
    MalformedMessageError,
  );
  assert.throws(
    () =>
      replies.subscription([
        { id: "subscription", values: [String(2n ** 40n)] },
      ]),
    MalformedMessageError,
  );
  assert.throws(
    () => replies.subscription([{ id: "other", values: ["1"] }]),
    MalformedMessageError,
  );
  assert.throws(
    () => parseTripletNotification([{ id: "other", values: [] }]),
    MalformedMessageError,
  );
});

test("a process transport forwards the push surface of its connection", () => {
  const listener = () => {};
  const stop = () => {};
  const transport = new ProcessTransport(
    {
      capabilities: {},
      stats: {},
      pendingPushes: 3,
      onPush: (given) => (given === listener ? stop : null),
      receivePush: (scope) => `push for ${scope}`,
    },
    null,
  );
  assert.equal(transport.onPush(listener), stop);
  assert.equal(transport.receivePush("scope"), "push for scope");
  assert.equal(transport.pendingPushes, 3);
});

/** Runs `action` and resolves to the uncaught error it causes, away from the test runner. */
async function uncaughtFrom(action) {
  const runnerHandlers = process.listeners("uncaughtException");
  process.removeAllListeners("uncaughtException");
  try {
    const uncaught = new Promise((resolve) =>
      process.once("uncaughtException", resolve),
    );
    action();
    return await uncaught;
  } finally {
    process.removeAllListeners("uncaughtException");
    for (const handler of runnerHandlers)
      process.on("uncaughtException", handler);
  }
}

/** A client of a server answering the first request with `messages`, unverified. */
async function clientReceiving(messages, t) {
  const server = createServer((socket) => {
    socket.once("data", () => {
      for (const message of messages) socket.write(encodeMessage(message));
    });
    socket.on("error", () => {});
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { address, port } = server.address();
  const client = await connectTcp(`${address}:${port}`, {
    verify: false,
    timeout: 5000,
  });
  t.after(async () => {
    await client.close();
    server.close();
  });
  return client;
}

const PUSH = "(notify: 1)\n() ((1: 0 0))";

test("a TCP push listener failure surfaces uncaught and leaves the connection open", async (t) => {
  const failure = new Error("the listener broke");
  const { transport } = await clientReceiving([PUSH, "(count: 0)"], t);
  const seen = [];
  const stop = transport.onPush((text) => {
    seen.push(text);
    throw failure;
  });
  assert.throws(() => transport.onPush("listener"), TypeError);
  let reply;
  const thrown = await uncaughtFrom(() => {
    reply = transport.send("(count: (0))", {});
  });
  assert.equal(thrown, failure);
  assert.equal(await reply, "(count: 0)");
  assert.deepEqual(seen, [PUSH]);
  stop();
});

test("a push the parser refuses breaks the connection instead of reading as a reply", async (t) => {
  const { transport } = await clientReceiving(["(notify: (1"], t);
  await assert.rejects(transport.send("(count: (0))", {}), LinksError);
});

test("a push waiter without a scope stays until a push arrives", async (t) => {
  const { transport } = await clientReceiving([PUSH, "(count: 0)"], t);
  const waiting = transport.receivePush();
  await transport.send("(count: (0))", {});
  assert.equal(await waiting, PUSH);
});

test("restriction shape, equal addresses, non-BigInt addresses and every null spelling", () => {
  assert.throws(() => checkRestriction("all"), TypeError);
  assert.throws(() => checkRestriction([1, 2, 3, 4]), TypeError);
  const same = [{ index: 2n }, { index: 1n }, { index: 2n }];
  assert.deepEqual(
    byIndex(same).map((link) => link.index),
    [1n, 2n, 2n],
  );
  const ids = [{ index: "b" }, { index: "a" }];
  assert.deepEqual(byIndex(ids), [{ index: "b" }, { index: "a" }]);
  for (const zero of [0n, 0, "0"]) assert.equal(isNullAddress(zero), true);
  for (const other of [1n, 1, "a"]) assert.equal(isNullAddress(other), false);
});

test("a binding read while its pair vanishes is absent, and the null link has no id", async () => {
  const ids = new LinksExternalIds("race");
  const store = new MemoryLinks();
  const target = await make(store, 1n, 1n);
  const pair = await ids.bind(store, "id", target);
  await assert.rejects(ids.bind(store, "null", 0n), TypeError);

  // The pair is removed after the scheme check accepted it.
  const each = store.each.bind(store);
  let reads = 0;
  store.each = async (restriction, options) => {
    if (restriction[0] === pair && (reads += 1) === 2) return [];
    return each(restriction, options);
  };
  assert.equal(await ids.find(store, "id"), null);
});

test("external ids ignore a scheme name that is not the shape of a scheme link", async () => {
  const ids = new LinksExternalIds("shape");
  const store = new MemoryLinks();
  const root = await make(store, 1n, 1n);
  await store.setName(root, "ExternalId");

  // Missing link, wrong source, wrong target: none reads as the scheme.
  await store.setName(999n, "shape");
  assert.equal(await ids.find(store, "id"), null);
  const wrongSource = await make(store, 5n, 5n);
  await store.setName(wrongSource, "shape");
  assert.equal(await ids.find(store, "id"), null);
  const wrongTarget = await make(store, root, 5n);
  await store.setName(wrongTarget, "shape");
  assert.equal(await ids.find(store, "id"), null);
});

test("an address table skips outer addresses added by hand when allocating", () => {
  const table = new AddressTable({ first: 100n });
  table.add(100n, 7n);
  table.add(101n, 8n);
  assert.equal(table.toOuter(9n), 102n);
  assert.equal(table.toOuter(7n), 100n);
});

test("a union stores a name of an unowned address in its primary and reports close failures", async () => {
  const primary = new MemoryLinks("primary");
  const other = new MemoryLinks("other");
  const owned = await make(other, 1n, 1n);
  await make(other, 1n, 1n);
  const union = new UnionLinks([primary, other]);
  await union.setName(owned + 1n, "owned elsewhere");
  assert.equal(await other.getName(owned + 1n), "owned elsewhere");
  await union.setName(50n, "unowned");
  assert.equal(await primary.getName(50n), "unowned");

  const failure = new Error("close failed");
  other.close = async () => {
    throw failure;
  };
  await assert.rejects(union.close(), failure);
  assert.equal(primary.closed, 1);
});

test("a push waiter with an already ended scope is refused without waiting", async (t) => {
  const { transport } = await clientReceiving([], t);
  await assert.rejects(transport.receivePush({ signal: AbortSignal.abort() }), {
    name: "AbortError",
    outcome: "not-sent",
  });
});
