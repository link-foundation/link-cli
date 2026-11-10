// A draft Raft replica that keeps the journals of an atomic journal store.
//
// A port of rust/src/engine/raft.rs. This is the algorithm etcd runs behind
// `EtcdJournalStore`, written as a pure state machine in the style of etcd's
// `raft` package: the node reads no clock, opens no socket and starts no timer.
// The caller passes the local time to `tick` and `step`, delivers the messages
// from `takeMessages` over any transport, and persists `hardState` and the log
// before sending them. That makes the same code usable in a real deployment and
// in a deterministic simulation, where tests drive it with skewed and jumping
// clocks, lost and reordered messages, partitions and restarts
// (test/engine-raft-simulation.test.mjs).
//
// Safety (election safety, log matching, leader completeness and state machine
// safety) never depends on a clock: time only decides *when* a node starts an
// election or a leader sends heartbeats. Clocks matter for liveness, and two
// etcd options keep a cluster live under faults that a plain Raft does not
// survive:
//
// * PreVote: a node first asks whether it *could* win before raising its term,
//   so a member that can send but not receive cannot disrupt a healthy leader
//   over and over.
// * CheckQuorum: a leader that has not heard from a majority within an election
//   timeout steps down, and followers that heard from a leader within that
//   timeout refuse votes (leader stickiness). A leader that can send but not
//   receive therefore stops blocking an election.
//
// Reads are linearizable because they go through the log (as etcd's older
// quorum reads did): a read returns the state at its position in the committed
// order. No proposal is retried; a proposal lost on the way to the leader simply
// never commits, and the caller's deadline reports an unknown outcome, as an
// atomic journal store requires.
//
// Journal versions match `ClusterEngine`: `{ generation, entries }` with the
// generation as a decimal string ('0' for a missing key), so the outcome of a
// read can serve `ClusterEngine.begin` directly. Terms, indexes, command ids
// and times are safe integers.
//
// This is a draft for users to try: membership changes, snapshots/log
// compaction and ReadIndex/lease reads are not implemented.

const MASK = (1n << 64n) - 1n;

/** splitmix64, used only to randomize election timeouts. */
class Jitter {
  #state;
  constructor(seed) {
    this.#state = BigInt(seed) & MASK;
  }
  below(bound) {
    this.#state = (this.#state + 0x9e3779b97f4a7c15n) & MASK;
    let z = this.#state;
    z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & MASK;
    z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & MASK;
    return Number((z ^ (z >> 31n)) % BigInt(Math.max(bound, 1)));
  }
}

export class RaftConfig {
  constructor(id, members) {
    /** This node's index in `0..members`. */
    this.id = id;
    this.members = members;
    /** The base election timeout in local clock units; each election waits a
     * random time in `[electionTimeout, 2 * electionTimeout)`. */
    this.electionTimeout = 100;
    this.heartbeatInterval = 10;
    this.preVote = true;
    this.checkQuorum = true;
    /** Seeds the election jitter (a number or a bigint, taken modulo 2^64). */
    this.seed = id + 1;
    /** The most entries one append message carries. */
    this.maxEntriesPerMessage = 32;
  }
}

export const RaftRole = Object.freeze({
  Follower: 'follower',
  PreCandidate: 'pre-candidate',
  Candidate: 'candidate',
  Leader: 'leader',
});

/** The replicated journals, as an atomic journal store exposes them. */
export class JournalState {
  /** Bumped by every successful compare-exchange; never reused. */
  revision = 0;
  /** key -> `{ generation, entries }`. */
  journals = new Map();
  /** `[revision, command id]` of every successful compare-exchange. */
  exchanges = [];
}

const copyVersion = ({ generation, entries }) => ({ generation, entries: [...entries] });

function copyState(state) {
  const copy = new JournalState();
  copy.revision = state.revision;
  for (const [key, version] of state.journals) copy.journals.set(key, copyVersion(version));
  copy.exchanges = state.exchanges.map((exchange) => [...exchange]);
  return copy;
}

/** Applies one committed command; a noop has no outcome. */
function applyCommand(state, command) {
  if (command.type === 'read')
    return {
      type: 'read',
      versions: command.keys.map((key) =>
        copyVersion(state.journals.get(key) ?? { generation: '0', entries: [] }),
      ),
    };
  if (command.type !== 'compareExchange') return undefined;
  const { id, keys, expected, updates } = command;
  if (
    keys.length !== expected.length ||
    keys.length !== updates.length ||
    keys.some((key, at) => (state.journals.get(key)?.generation ?? '0') !== expected[at].generation)
  )
    return { type: 'exchanged', applied: false };
  state.revision++;
  keys.forEach((key, at) => {
    if (updates[at] !== null)
      state.journals.set(key, { generation: String(state.revision), entries: [...updates[at]] });
  });
  state.exchanges.push([state.revision, id]);
  return { type: 'exchanged', applied: true };
}

const safe = (value, minimum) => Number.isSafeInteger(value) && value >= minimum;

export class RaftNode {
  #config;
  #jitter;
  // Persistent.
  #hard = { term: 0, votedFor: null };
  /** `log[0]` is a sentinel of term 0. */
  #log = [{ term: 0, command: { type: 'noop' } }];
  // Volatile.
  #role = RaftRole.Follower;
  #leader = null;
  #commit = 0;
  #applied = 0;
  #state = new JournalState();
  #votes = new Set();
  #next;
  #matched;
  /** Peers heard from since the last quorum check. */
  #active = new Set();
  #electionDue = 0;
  #heartbeatDue = 0;
  #quorumDue = 0;
  /** When this node last heard from a current leader. */
  #leaderHeard = null;
  #waiting = new Set();
  #outcomes = new Map();
  #outbox = [];

  constructor(config, now) {
    const { id, members, electionTimeout, heartbeatInterval, maxEntriesPerMessage } = config;
    if (!safe(members, 1) || !safe(id, 0) || id >= members)
      throw new RangeError('a Raft node id must be below a positive member count');
    if (
      !safe(heartbeatInterval, 0) ||
      !safe(electionTimeout, heartbeatInterval + 1) ||
      !safe(maxEntriesPerMessage, 0)
    )
      throw new RangeError(
        'the election timeout must exceed the heartbeat interval, both safe integers',
      );
    this.#config = {
      id,
      members,
      electionTimeout,
      heartbeatInterval,
      preVote: Boolean(config.preVote),
      checkQuorum: Boolean(config.checkQuorum),
      maxEntriesPerMessage,
    };
    this.#jitter = new Jitter(config.seed);
    this.#next = Array(members).fill(1);
    this.#matched = Array(members).fill(0);
    this.#resetElection(now);
  }

  /** A process restart: the hard state and the log survive, everything else
   * (commit index, applied state, pending outcomes) is rebuilt. */
  restart(now) {
    this.#role = RaftRole.Follower;
    this.#leader = null;
    this.#commit = 0;
    this.#applied = 0;
    this.#state = new JournalState();
    this.#votes.clear();
    this.#active.clear();
    this.#leaderHeard = null;
    this.#waiting.clear();
    this.#outcomes.clear();
    this.#outbox = [];
    this.#resetElection(now);
  }

  get id() {
    return this.#config.id;
  }
  get role() {
    return this.#role;
  }
  get term() {
    return this.#hard.term;
  }
  /** The leader this node knows, or null. */
  get leader() {
    return this.#leader;
  }
  get hardState() {
    return { ...this.#hard };
  }
  get commitIndex() {
    return this.#commit;
  }
  get appliedIndex() {
    return this.#applied;
  }
  /** The index of the last log entry. */
  get lastIndex() {
    return this.#log.length - 1;
  }
  /** A copy of the whole log, sentinel included. */
  get log() {
    return [...this.#log];
  }
  /** A copy of the applied journals. */
  get state() {
    return copyState(this.#state);
  }
  /** Messages to deliver; persist the hard state and log first. */
  takeMessages() {
    const messages = this.#outbox;
    this.#outbox = [];
    return messages;
  }
  /** The outcome of a command proposed on this node, once applied here. */
  takeOutcome(id) {
    const outcome = this.#outcomes.get(id);
    this.#outcomes.delete(id);
    return outcome;
  }
  /** Gives up waiting for a command (its caller timed out). */
  forget(id) {
    this.#waiting.delete(id);
    this.#outcomes.delete(id);
  }

  /** Proposes a command. Returns false when no leader is known, so the
   * proposal went nowhere. Even on true the proposal may be lost. */
  propose(_now, command) {
    if (command.type !== 'noop') this.#waiting.add(command.id);
    if (this.#role === RaftRole.Leader) {
      this.#appendLocal(command);
      this.#broadcastAppend();
      return true;
    }
    if (this.#leader !== null) {
      this.#send(this.#leader, { type: 'propose', command });
      return true;
    }
    return false;
  }

  /** Advances timers to the local time `now`. A clock that jumps backward only
   * delays them. */
  tick(now) {
    if (this.#role === RaftRole.Leader) {
      if (this.#config.checkQuorum && now >= this.#quorumDue) {
        this.#quorumDue = now + this.#config.electionTimeout;
        const heard = this.#active.size + 1;
        this.#active.clear();
        if (heard < this.#quorum()) {
          this.#becomeFollower(now, this.#hard.term, null);
          return;
        }
      }
      if (now >= this.#heartbeatDue) {
        this.#heartbeatDue = now + this.#config.heartbeatInterval;
        this.#broadcastAppend();
      }
    } else if (now >= this.#electionDue) this.#campaign(now, this.#config.preVote);
  }

  step(now, { from, message }) {
    if (!(Number.isSafeInteger(from) && from >= 0 && from < this.#config.members)) return;
    if (from === this.#config.id) return;
    if (message.type !== 'propose') {
      const { term } = message;
      if (term > this.#hard.term) {
        // Leader stickiness: ignore disruptive campaigns.
        if (message.type === 'vote' && this.#inLease(now)) return;
        const preRequest = message.type === 'vote' && message.pre;
        const preGrant = message.type === 'voteReply' && message.pre && message.granted;
        if (!preRequest && !preGrant)
          this.#becomeFollower(now, term, message.type === 'append' ? from : null);
      } else if (term < this.#hard.term) {
        // Tell a stale leader or candidate about the newer term.
        if (message.type === 'append')
          this.#send(from, {
            type: 'appendReply',
            term: this.#hard.term,
            success: false,
            index: 0,
          });
        else if (message.type === 'vote')
          this.#send(from, {
            type: 'voteReply',
            pre: message.pre,
            term: this.#hard.term,
            granted: false,
          });
        return;
      }
    }
    switch (message.type) {
      case 'vote':
        this.#onVote(now, from, message);
        break;
      case 'voteReply':
        this.#onVoteReply(now, from, message);
        break;
      case 'append':
        this.#onAppend(now, from, message);
        break;
      case 'appendReply':
        this.#onAppendReply(from, message);
        break;
      case 'propose':
        // Otherwise the proposal is dropped: no proposal is retried.
        if (this.#role === RaftRole.Leader) {
          this.#appendLocal(message.command);
          this.#broadcastAppend();
        }
    }
  }

  #quorum() {
    return Math.floor(this.#config.members / 2) + 1;
  }
  #lastTerm() {
    return this.#log[this.#log.length - 1].term;
  }
  #inLease(now) {
    // A follower that knows a leader has heard from it.
    return (
      this.#config.checkQuorum &&
      (this.#role === RaftRole.Leader ||
        (this.#leader !== null && now < this.#leaderHeard + this.#config.electionTimeout))
    );
  }
  #send(to, message) {
    this.#outbox.push({ from: this.#config.id, to, message });
  }
  #resetElection(now) {
    this.#electionDue =
      now + this.#config.electionTimeout + this.#jitter.below(this.#config.electionTimeout);
  }
  #becomeFollower(now, term, leader) {
    if (term > this.#hard.term) {
      this.#hard.term = term;
      this.#hard.votedFor = null;
    }
    this.#role = RaftRole.Follower;
    this.#leader = leader;
    this.#votes.clear();
    if (leader !== null) this.#leaderHeard = now;
    this.#resetElection(now);
  }
  #campaign(now, pre) {
    this.#resetElection(now);
    this.#votes.clear();
    this.#votes.add(this.#config.id);
    let term;
    if (pre) {
      this.#role = RaftRole.PreCandidate;
      term = this.#hard.term + 1;
    } else {
      this.#role = RaftRole.Candidate;
      this.#hard.term++;
      this.#hard.votedFor = this.#config.id;
      term = this.#hard.term;
    }
    this.#leader = null;
    if (this.#votes.size >= this.#quorum()) {
      this.#win(now, pre);
      return;
    }
    const lastIndex = this.lastIndex;
    const lastTerm = this.#lastTerm();
    for (let peer = 0; peer < this.#config.members; peer++)
      if (peer !== this.#config.id)
        this.#send(peer, { type: 'vote', pre, term, lastIndex, lastTerm });
  }
  #win(now, pre) {
    if (pre) {
      this.#campaign(now, false);
      return;
    }
    this.#role = RaftRole.Leader;
    this.#leader = this.#config.id;
    this.#votes.clear();
    this.#active.clear();
    this.#next.fill(this.lastIndex + 1);
    this.#matched.fill(0);
    this.#quorumDue = now + this.#config.electionTimeout;
    this.#heartbeatDue = now + this.#config.heartbeatInterval;
    this.#appendLocal({ type: 'noop' });
    this.#broadcastAppend();
  }
  #onVote(now, from, { pre, term, lastIndex, lastTerm }) {
    const upToDate =
      lastTerm > this.#lastTerm() || (lastTerm === this.#lastTerm() && lastIndex >= this.lastIndex);
    const granted = pre
      ? // A pre-vote never changes this node's state.
        term > this.#hard.term && upToDate && !this.#inLease(now)
      : term === this.#hard.term &&
        upToDate &&
        (this.#hard.votedFor === null || this.#hard.votedFor === from);
    if (granted && !pre) {
      this.#hard.votedFor = from;
      this.#resetElection(now);
    }
    this.#send(from, {
      type: 'voteReply',
      pre,
      term: pre && granted ? term : this.#hard.term,
      granted,
    });
  }
  #onVoteReply(now, from, { pre, term, granted }) {
    let expected;
    if (this.#role === RaftRole.PreCandidate && pre) expected = this.#hard.term + 1;
    else if (this.#role === RaftRole.Candidate && !pre) expected = this.#hard.term;
    else return;
    if (term !== expected || !granted) return;
    this.#votes.add(from);
    if (this.#votes.size >= this.#quorum()) this.#win(now, pre);
  }
  #onAppend(now, from, { prevIndex, prevTerm, entries, commit }) {
    // The term equals ours here; a candidate of this term steps down.
    if (this.#role !== RaftRole.Follower || this.#leader !== from) {
      this.#role = RaftRole.Follower;
      this.#leader = from;
      this.#votes.clear();
    }
    this.#leaderHeard = now;
    this.#resetElection(now);
    if (prevIndex > this.lastIndex || this.#log[prevIndex].term !== prevTerm) {
      this.#send(from, {
        type: 'appendReply',
        term: this.#hard.term,
        success: false,
        index: Math.min(Math.max(prevIndex - 1, 0), this.lastIndex),
      });
      return;
    }
    let index = prevIndex;
    for (const entry of entries) {
      index++;
      if (index <= this.lastIndex) {
        if (this.#log[index].term === entry.term) continue;
        // A conflicting suffix was never committed: drop it.
        if (index <= this.#commit) throw new Error('a committed entry would be overwritten');
        this.#log.length = index;
      }
      this.#log.push(entry);
    }
    if (commit > this.#commit) {
      this.#commit = Math.min(commit, index);
      this.#apply();
    }
    this.#send(from, { type: 'appendReply', term: this.#hard.term, success: true, index });
  }
  #onAppendReply(from, { success, index }) {
    if (this.#role !== RaftRole.Leader) return;
    this.#active.add(from);
    if (success) {
      if (index > this.#matched[from]) this.#matched[from] = index;
      this.#next[from] = Math.max(this.#next[from], index + 1);
      this.#advanceCommit();
      if (this.#next[from] <= this.lastIndex) this.#sendAppend(from);
    } else {
      this.#next[from] = Math.max(Math.min(index + 1, this.#next[from] - 1), 1);
      this.#sendAppend(from);
    }
  }
  #appendLocal(command) {
    this.#log.push({ term: this.#hard.term, command });
    const id = this.#config.id;
    this.#matched[id] = this.lastIndex;
    this.#next[id] = this.lastIndex + 1;
    this.#advanceCommit();
  }
  #broadcastAppend() {
    for (let peer = 0; peer < this.#config.members; peer++)
      if (peer !== this.#config.id) this.#sendAppend(peer);
  }
  #sendAppend(peer) {
    const prevIndex = this.#next[peer] - 1;
    const end = Math.min(this.#log.length, prevIndex + 1 + this.#config.maxEntriesPerMessage);
    this.#send(peer, {
      type: 'append',
      term: this.#hard.term,
      prevIndex,
      prevTerm: this.#log[prevIndex].term,
      entries: this.#log.slice(prevIndex + 1, end),
      commit: this.#commit,
    });
  }
  #advanceCommit() {
    const matched = [...this.#matched].sort((a, b) => b - a);
    const candidate = matched[this.#quorum() - 1];
    // Only entries of the current term commit by counting replicas.
    if (candidate > this.#commit && this.#log[candidate].term === this.#hard.term) {
      this.#commit = candidate;
      this.#apply();
    }
  }
  #apply() {
    while (this.#applied < this.#commit) {
      this.#applied++;
      const { command } = this.#log[this.#applied];
      const outcome = applyCommand(this.#state, command);
      // Only commands with ids have outcomes.
      if (outcome !== undefined && this.#waiting.delete(command.id))
        this.#outcomes.set(command.id, outcome);
    }
  }
}
