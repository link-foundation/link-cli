import assert from 'node:assert/strict';
import test from 'node:test';

import { JournalState, RaftConfig, RaftNode, RaftRole } from '../src/raft.js';

const EMPTY = { generation: '0', entries: [] };

function deliver(nodes, now) {
  for (;;) {
    const messages = nodes.flatMap((node) => node.takeMessages());
    if (messages.length === 0) return;
    for (const message of messages) nodes[message.to].step(now, message);
  }
}

function member(id, members, options = {}) {
  const config = new RaftConfig(id, members);
  Object.assign(config, options);
  return new RaftNode(config, 0);
}

function cluster(preVote, checkQuorum) {
  return [0, 1, 2].map((id) => member(id, 3, { preVote, checkQuorum }));
}

const read = (id, keys) => ({ type: 'read', id, keys });
const exchange = (id, keys, expected, updates) => ({
  type: 'compareExchange',
  id,
  keys,
  expected,
  updates,
});
const entry = (term, command = { type: 'noop' }) => ({ term, command });
const append = (term, prevIndex, prevTerm, entries = [], commit = 0) => ({
  type: 'append',
  term,
  prevIndex,
  prevTerm,
  entries,
  commit,
});
const vote = (pre, term, lastIndex = 0, lastTerm = 0) => ({
  type: 'vote',
  pre,
  term,
  lastIndex,
  lastTerm,
});

test('a single member elects itself and commits', () => {
  const node = member(0, 1);
  assert.equal(node.id, 0);
  node.tick(1000);
  assert.equal(node.role, RaftRole.Leader);
  assert.equal(node.leader, 0);
  assert.deepEqual(node.hardState, { term: 1, votedFor: 0 });
  assert.equal(node.propose(1000, read(7, ['a'])), true);
  assert.deepEqual(node.takeOutcome(7), { type: 'read', versions: [EMPTY] });
  assert.equal(node.takeOutcome(7), undefined);
  // A noop needs no outcome.
  assert.equal(node.propose(1000, { type: 'noop' }), true);
  assert.equal(node.commitIndex, 3);
  assert.equal(node.appliedIndex, 3);
  assert.equal(node.lastIndex, 3);
  assert.deepEqual(
    node.log.map(({ term }) => term),
    [0, 1, 1, 1],
  );
  assert.deepEqual(node.takeMessages(), []);
});

test('a leader replicates compare-exchange to every member', () => {
  const nodes = cluster(true, true);
  nodes[0].tick(1000);
  deliver(nodes, 1000);
  assert.equal(nodes[0].role, RaftRole.Leader);
  const keys = ['a'];
  assert.equal(nodes[1].propose(1001, exchange(1, keys, [EMPTY], [['x']])), true);
  deliver(nodes, 1001);
  assert.equal(nodes[0].commitIndex, 2);
  // Followers learn the commit index from the next heartbeat.
  assert.equal(nodes[1].takeOutcome(1), undefined);
  nodes[0].tick(1011);
  deliver(nodes, 1011);
  assert.deepEqual(nodes[1].takeOutcome(1), { type: 'exchanged', applied: true });
  for (const node of nodes) assert.deepEqual(node.state.journals.get('a').entries, ['x']);
  // The same compare-exchange again sees a newer generation.
  assert.equal(nodes[2].propose(1012, exchange(2, keys, [EMPTY], [['x']])), true);
  deliver(nodes, 1012);
  nodes[0].tick(1022);
  deliver(nodes, 1022);
  assert.deepEqual(nodes[2].takeOutcome(2), { type: 'exchanged', applied: false });
  assert.deepEqual(nodes[0].state.exchanges, [[1, 1]]);
});

test('a member inside a leader lease ignores a disruptive campaign', () => {
  const nodes = cluster(true, true);
  nodes[0].tick(1000);
  deliver(nodes, 1000);
  // Node 2 cannot hear anything, times out and campaigns.
  nodes[2].tick(5000);
  const campaign = nodes[2].takeMessages();
  assert.ok(campaign.every(({ message }) => message.type === 'vote' && message.pre));
  for (const message of campaign) nodes[message.to].step(1005, message);
  // Neither member replies or raises its term.
  assert.deepEqual([...nodes[0].takeMessages(), ...nodes[1].takeMessages()], []);
  assert.deepEqual([nodes[0].term, nodes[0].role], [1, RaftRole.Leader]);
  assert.equal(nodes[2].term, 1);
});

test('a leader that hears no majority steps down under check quorum', () => {
  const nodes = cluster(true, true);
  nodes[0].tick(1000);
  deliver(nodes, 1000);
  nodes[0].tick(1100);
  assert.equal(nodes[0].role, RaftRole.Leader);
  nodes[0].takeMessages();
  nodes[0].tick(1200);
  assert.equal(nodes[0].role, RaftRole.Follower);
  assert.equal(nodes[0].leader, null);
});

test('a leader without check quorum never steps down', () => {
  const nodes = cluster(false, false);
  nodes[0].tick(1000);
  deliver(nodes, 1000);
  nodes[0].tick(5000);
  nodes[0].tick(9000);
  assert.equal(nodes[0].role, RaftRole.Leader);
});

test('a restarted member rebuilds its state from the log', () => {
  const nodes = cluster(false, false);
  nodes[0].tick(1000);
  deliver(nodes, 1000);
  assert.equal(nodes[0].role, RaftRole.Leader);
  nodes[0].propose(1000, exchange(1, ['a'], [EMPTY], [['x']]));
  deliver(nodes, 1000);
  nodes[1].restart(1001);
  assert.deepEqual(nodes[1].state, new JournalState());
  assert.equal(nodes[1].role, RaftRole.Follower);
  nodes[0].tick(1011);
  deliver(nodes, 1011);
  assert.deepEqual(nodes[1].state, nodes[0].state);
  assert.equal(nodes[1].hardState.term, 1);
});

test('the journal state applies reads and compare-exchanges', () => {
  const node = member(0, 1);
  node.tick(1000);
  const outcome = (command) => {
    node.propose(1000, command);
    return node.takeOutcome(command.id);
  };
  const failed = { type: 'exchanged', applied: false };
  const applied = { type: 'exchanged', applied: true };
  assert.deepEqual(outcome(exchange(1, ['a'], [], [null])), failed);
  assert.deepEqual(outcome(exchange(2, ['a'], [EMPTY], [])), failed);
  assert.deepEqual(outcome(exchange(3, ['a', 'b'], [EMPTY, EMPTY], [['x'], null])), applied);
  assert.deepEqual(outcome(exchange(4, ['a'], [EMPTY], [['y']])), failed);
  const first = { generation: '1', entries: ['x'] };
  assert.deepEqual(outcome(read(5, ['a', 'b'])), { type: 'read', versions: [first, EMPTY] });
  assert.deepEqual(outcome(exchange(6, ['a', 'b'], [first, EMPTY], [null, ['z']])), applied);
  const { state } = node;
  assert.equal(state.revision, 2);
  assert.deepEqual(
    [...state.journals],
    [
      ['a', first],
      ['b', { generation: '2', entries: ['z'] }],
    ],
  );
  assert.deepEqual(state.exchanges, [
    [1, 3],
    [2, 6],
  ]);
  // Outcomes and the state are copies.
  state.journals.get('a').entries.push('changed');
  assert.deepEqual(node.state.journals.get('a'), first);
});

test('a configuration is validated', () => {
  const invalid = (id, members, options) =>
    assert.throws(() => member(id, members, options), RangeError);
  invalid(0, 0);
  invalid(-1, 3);
  invalid(3, 3);
  invalid(0, 3, { heartbeatInterval: -1 });
  invalid(0, 3, { electionTimeout: 10, heartbeatInterval: 10 });
  invalid(0, 3, { maxEntriesPerMessage: 1.5 });
  // A bigint seed is taken modulo 2^64.
  const wide = member(0, 1, { seed: (1n << 64n) + 1n });
  const narrow = member(0, 1, { seed: 1 });
  for (let now = 100; now < 200; now++) {
    for (const node of [wide, narrow]) node.tick(now);
    assert.equal(wide.role, narrow.role);
  }
  assert.equal(wide.role, RaftRole.Leader);
});

test('messages from unknown members or from itself are ignored', () => {
  const node = member(1, 3);
  for (const from of ['0', -1, 3, 0.5, 1]) node.step(0, { from, to: 1, message: append(5, 0, 0) });
  assert.equal(node.term, 0);
  assert.deepEqual(node.takeMessages(), []);
});

test('stale messages are answered with the newer term', () => {
  const node = member(1, 3);
  node.step(0, { from: 0, to: 1, message: append(2, 0, 0) });
  assert.deepEqual([node.term, node.leader, node.role], [2, 0, RaftRole.Follower]);
  node.takeMessages();
  node.step(0, { from: 2, to: 1, message: append(1, 0, 0) });
  node.step(0, { from: 2, to: 1, message: vote(true, 1) });
  node.step(0, {
    from: 2,
    to: 1,
    message: { type: 'voteReply', pre: false, term: 1, granted: true },
  });
  node.step(0, {
    from: 2,
    to: 1,
    message: { type: 'appendReply', term: 1, success: true, index: 0 },
  });
  assert.deepEqual(
    node.takeMessages().map(({ to, message }) => [to, message]),
    [
      [2, { type: 'appendReply', term: 2, success: false, index: 0 }],
      [2, { type: 'voteReply', pre: true, term: 2, granted: false }],
    ],
  );
  assert.equal(node.term, 2);
});

test('a newer term turns a member into a follower', () => {
  const node = member(1, 3, { checkQuorum: false });
  node.step(0, { from: 0, to: 1, message: append(1, 0, 0) });
  // Without check quorum there is no lease: a real vote of a newer term wins.
  node.step(1, { from: 2, to: 1, message: vote(false, 3) });
  assert.deepEqual(node.hardState, { term: 3, votedFor: 2 });
  assert.equal(node.leader, null);
  // A pre-vote raises no term and is granted with the requested term.
  node.step(2, { from: 0, to: 1, message: vote(true, 5) });
  assert.equal(node.term, 3);
  // A refused vote reply or an append reply of a newer term still steps down.
  node.step(3, {
    from: 0,
    to: 1,
    message: { type: 'voteReply', pre: true, term: 6, granted: false },
  });
  assert.equal(node.term, 6);
  node.step(4, {
    from: 0,
    to: 1,
    message: { type: 'appendReply', term: 7, success: true, index: 0 },
  });
  assert.deepEqual(node.hardState, { term: 7, votedFor: null });
  assert.deepEqual(
    node.takeMessages().map(({ message }) => message),
    [
      { type: 'appendReply', term: 1, success: true, index: 0 },
      { type: 'voteReply', pre: false, term: 3, granted: true },
      { type: 'voteReply', pre: true, term: 5, granted: true },
    ],
  );
});

test('votes go only to up-to-date candidates, once per term', () => {
  const node = member(1, 3, { checkQuorum: false });
  node.step(0, { from: 0, to: 1, message: append(2, 0, 0, [entry(1), entry(2)]) });
  node.takeMessages();
  const replies = () => node.takeMessages().map(({ message }) => message.granted);
  // Older last term, shorter log of the same term, then up to date.
  node.step(1, { from: 2, to: 1, message: vote(false, 3, 5, 1) });
  node.step(1, { from: 2, to: 1, message: vote(false, 3, 1, 2) });
  assert.deepEqual(replies(), [false, false]);
  node.step(1, { from: 2, to: 1, message: vote(false, 3, 2, 2) });
  node.step(1, { from: 2, to: 1, message: vote(false, 3, 2, 2) });
  node.step(1, { from: 0, to: 1, message: vote(false, 3, 9, 3) });
  assert.deepEqual(replies(), [true, true, false]);
  // A pre-vote for a stale log or the current term is refused.
  node.step(1, { from: 0, to: 1, message: vote(true, 4, 1, 1) });
  node.step(1, { from: 0, to: 1, message: vote(true, 3, 9, 3) });
  assert.deepEqual(replies(), [false, false]);
});

test('a pre-vote is refused inside a lease and granted after it', () => {
  const node = member(1, 3);
  node.step(0, { from: 0, to: 1, message: append(1, 0, 0) });
  node.takeMessages();
  // Inside the lease the campaign gets no reply at all.
  node.step(50, { from: 2, to: 1, message: vote(true, 2) });
  assert.deepEqual(node.takeMessages(), []);
  node.step(150, { from: 2, to: 1, message: vote(true, 2) });
  assert.deepEqual(
    node.takeMessages().map(({ message }) => [message.term, message.granted]),
    [[2, true]],
  );
  assert.equal(node.term, 1);
});

test('a candidate needs a majority of matching replies', () => {
  const reply = (pre, term, granted = true) => ({ type: 'voteReply', pre, term, granted });
  const node = member(0, 5);
  // A follower ignores vote replies of its term.
  node.step(0, { from: 1, to: 0, message: reply(false, 0) });
  node.tick(1000);
  assert.deepEqual([node.role, node.term], [RaftRole.PreCandidate, 0]);
  assert.equal(node.takeMessages().length, 4);
  node.step(1000, { from: 1, to: 0, message: reply(false, 0) });
  node.step(1000, { from: 1, to: 0, message: reply(true, 2) });
  node.step(1000, { from: 1, to: 0, message: reply(true, 1) });
  node.step(1000, { from: 1, to: 0, message: reply(true, 1) });
  assert.equal(node.role, RaftRole.PreCandidate);
  node.step(1000, { from: 2, to: 0, message: reply(true, 1) });
  assert.deepEqual([node.role, node.term], [RaftRole.Candidate, 1]);
  node.step(1000, { from: 3, to: 0, message: reply(true, 2) });
  node.step(1000, { from: 3, to: 0, message: reply(false, 1, false) });
  node.step(1000, { from: 4, to: 0, message: reply(false, 1) });
  assert.equal(node.role, RaftRole.Candidate);
  node.step(1000, { from: 1, to: 0, message: reply(false, 1) });
  assert.deepEqual([node.role, node.leader], [RaftRole.Leader, 0]);
});

test('an append of the current term makes a candidate follow its sender', () => {
  const node = member(1, 3, { preVote: false });
  node.tick(1000);
  assert.deepEqual([node.role, node.term], [RaftRole.Candidate, 1]);
  node.step(1000, { from: 2, to: 1, message: append(1, 0, 0) });
  assert.deepEqual([node.role, node.leader], [RaftRole.Follower, 2]);
  node.step(1001, { from: 2, to: 1, message: append(1, 0, 0) });
  assert.equal(node.leader, 2);
});

test('a follower repairs its log from the leader', () => {
  const node = member(1, 3);
  const replies = () => node.takeMessages().map(({ message }) => [message.success, message.index]);
  const read1 = entry(1, read(1, ['a']));
  node.step(0, { from: 0, to: 1, message: append(1, 0, 0, [entry(1), read1], 1) });
  node.step(0, { from: 0, to: 1, message: append(1, 4, 1) });
  node.step(0, { from: 0, to: 1, message: append(1, 2, 2) });
  node.step(0, { from: 0, to: 1, message: append(1, 0, 1) });
  assert.deepEqual(replies(), [
    [true, 2],
    [false, 2],
    [false, 1],
    [false, 0],
  ]);
  assert.equal(node.commitIndex, 1);
  // A new leader overwrites the uncommitted entry; a repeated append is a no-op.
  const replacement = [entry(2), entry(2, read(2, ['b']))];
  node.step(1, { from: 2, to: 1, message: append(2, 1, 1, replacement, 9) });
  node.step(1, { from: 2, to: 1, message: append(2, 1, 1, replacement, 3) });
  assert.deepEqual(replies(), [
    [true, 3],
    [true, 3],
  ]);
  assert.deepEqual(
    node.log.map(({ term }) => term),
    [0, 1, 2, 2],
  );
  // The commit index never passes the entries this append verified.
  assert.equal(node.commitIndex, 3);
  // Outcomes are kept only for commands proposed on this node.
  assert.equal(node.takeOutcome(2), undefined);
  assert.throws(
    () => node.step(2, { from: 2, to: 1, message: append(3, 0, 0, [entry(3)]) }),
    /a committed entry would be overwritten/,
  );
});

test('a new leader commits only entries of its own term by counting', () => {
  const node = member(0, 3, { preVote: false, checkQuorum: false, maxEntriesPerMessage: 1 });
  node.step(0, { from: 1, to: 0, message: append(1, 0, 0, [entry(1)]) });
  // A follower ignores append replies.
  node.step(0, {
    from: 1,
    to: 0,
    message: { type: 'appendReply', term: 1, success: true, index: 1 },
  });
  node.tick(1000);
  node.takeMessages();
  node.step(1000, {
    from: 2,
    to: 0,
    message: { type: 'voteReply', pre: false, term: 2, granted: true },
  });
  assert.equal(node.role, RaftRole.Leader);
  const sent = () =>
    node.takeMessages().map(({ to, message }) => [to, message.prevIndex, message.entries.length]);
  assert.deepEqual(sent(), [
    [1, 1, 1],
    [2, 1, 1],
  ]);
  const reply = (from, success, index) =>
    node.step(1000, { from, to: 0, message: { type: 'appendReply', term: 2, success, index } });
  reply(2, false, 0);
  assert.deepEqual(sent(), [[2, 0, 1]]);
  reply(2, true, 1);
  assert.equal(node.commitIndex, 0);
  assert.deepEqual(sent(), [[2, 1, 1]]);
  reply(2, true, 2);
  assert.equal(node.commitIndex, 2);
  reply(2, true, 1);
  assert.deepEqual(sent(), []);
});

test('proposals reach the leader or report that none is known', () => {
  const nodes = cluster(true, true);
  assert.equal(nodes[1].propose(0, read(1, [])), false);
  nodes[0].tick(1000);
  deliver(nodes, 1000);
  assert.equal(nodes[1].propose(1000, read(2, [])), true);
  const [forwarded] = nodes[1].takeMessages();
  assert.deepEqual(forwarded, {
    from: 1,
    to: 0,
    message: { type: 'propose', command: read(2, []) },
  });
  // A follower drops a forwarded proposal.
  nodes[2].step(1000, { ...forwarded, to: 2 });
  assert.deepEqual(nodes[2].takeMessages(), []);
  nodes[0].step(1000, forwarded);
  // A forgotten command keeps no outcome.
  nodes[1].forget(2);
  deliver(nodes, 1000);
  nodes[0].tick(1010);
  deliver(nodes, 1010);
  assert.equal(nodes[1].appliedIndex, 2);
  assert.equal(nodes[1].takeOutcome(2), undefined);
  // A follower waits for its election timeout.
  nodes[1].tick(1011);
  assert.equal(nodes[1].role, RaftRole.Follower);
});
