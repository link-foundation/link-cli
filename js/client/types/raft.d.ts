import type { JournalVersion } from './index.js';

/** Configuration of one replica; every field may be changed before `new RaftNode`. */
export declare class RaftConfig {
  constructor(id: number, members: number);
  /** This node's index in `0..members`. */
  id: number;
  members: number;
  /** Base election timeout in local clock units; each election waits in `[t, 2t)`. Default 100. */
  electionTimeout: number;
  /** Default 10; must stay below the election timeout. */
  heartbeatInterval: number;
  /** Ask whether an election could be won before raising the term. Default true. */
  preVote: boolean;
  /** A leader that hears no majority within an election timeout steps down,
   * and followers in a leader's lease refuse votes. Default true. */
  checkQuorum: boolean;
  /** Seeds the election jitter, modulo 2^64. Default `id + 1`. */
  seed: number | bigint;
  /** The most entries one append message carries. Default 32. */
  maxEntriesPerMessage: number;
}

export declare const RaftRole: Readonly<{
  Follower: 'follower';
  PreCandidate: 'pre-candidate';
  Candidate: 'candidate';
  Leader: 'leader';
}>;
export type RaftRole = (typeof RaftRole)[keyof typeof RaftRole];

/** A replicated command. `null` in `updates` leaves a key unchanged. */
export type RaftCommand =
  | { type: 'noop' }
  | { type: 'read'; id: number; keys: string[] }
  | {
      type: 'compareExchange';
      id: number;
      keys: string[];
      expected: JournalVersion[];
      updates: (string[] | null)[];
    };
export type RaftOutcome =
  { type: 'read'; versions: JournalVersion[] } | { type: 'exchanged'; applied: boolean };
export interface RaftEntry {
  term: number;
  command: RaftCommand;
}
export type RaftMessage =
  | { type: 'vote'; pre: boolean; term: number; lastIndex: number; lastTerm: number }
  | { type: 'voteReply'; pre: boolean; term: number; granted: boolean }
  | {
      type: 'append';
      term: number;
      prevIndex: number;
      prevTerm: number;
      entries: RaftEntry[];
      commit: number;
    }
  | { type: 'appendReply'; term: number; success: boolean; index: number }
  | { type: 'propose'; command: RaftCommand };
export interface RaftEnvelope {
  from: number;
  to: number;
  message: RaftMessage;
}
/** Persist before sending the messages of the same step. */
export interface RaftHardState {
  term: number;
  votedFor: number | null;
}

/** The replicated journals, as an atomic journal store exposes them. */
export declare class JournalState {
  /** Bumped by every successful compare-exchange; never reused. */
  revision: number;
  journals: Map<string, JournalVersion>;
  /** `[revision, command id]` of every successful compare-exchange. */
  exchanges: [number, number][];
}

/** A draft etcd-style Raft replica: a pure state machine without clocks or I/O. */
export declare class RaftNode {
  /** Throws a RangeError for an invalid configuration. */
  constructor(config: RaftConfig, now: number);
  /** A process restart: the hard state and the log survive. */
  restart(now: number): void;
  readonly id: number;
  readonly role: RaftRole;
  readonly term: number;
  readonly leader: number | null;
  readonly hardState: RaftHardState;
  readonly commitIndex: number;
  readonly appliedIndex: number;
  readonly lastIndex: number;
  /** A copy of the log; `log[0]` is a sentinel of term 0. */
  readonly log: RaftEntry[];
  /** A copy of the applied journals. */
  readonly state: JournalState;
  takeMessages(): RaftEnvelope[];
  takeOutcome(id: number): RaftOutcome | undefined;
  forget(id: number): void;
  /** False when no leader is known; true does not mean the command commits. */
  propose(now: number, command: RaftCommand): boolean;
  tick(now: number): void;
  step(now: number, envelope: RaftEnvelope): void;
}
