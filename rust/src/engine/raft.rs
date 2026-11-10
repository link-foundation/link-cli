//! A draft Raft replica that keeps the journals of an `AtomicJournalStore`.
//!
//! This is the algorithm etcd runs behind `EtcdJournalStore`, written as a
//! pure state machine in the style of etcd's `raft` package: the node reads no
//! clock, opens no socket and starts no thread. The caller passes the local
//! time to [`RaftNode::tick`] and [`RaftNode::step`], delivers the messages
//! from [`RaftNode::take_messages`] over any transport, and persists
//! [`RaftNode::hard_state`] and the log before sending them. That makes the
//! same code usable in a real deployment and in a deterministic simulation,
//! where tests drive it with skewed and jumping clocks, lost and reordered
//! messages, partitions and restarts (`engine_raft_simulation_tests.rs`).
//!
//! Safety (election safety, log matching, leader completeness and state
//! machine safety) never depends on a clock: time only decides *when* a node
//! starts an election or a leader sends heartbeats. Clocks matter for
//! liveness, and two etcd options keep a cluster live under faults that a
//! plain Raft does not survive:
//!
//! * **PreVote** — a node first asks whether it *could* win before raising
//!   its term, so a member that can send but not receive cannot disrupt a
//!   healthy leader over and over.
//! * **CheckQuorum** — a leader that has not heard from a majority within an
//!   election timeout steps down, and followers that heard from a leader
//!   within that timeout refuse votes (leader stickiness). A leader that can
//!   send but not receive therefore stops blocking an election.
//!
//! Reads are linearizable because they go through the log (as etcd's older
//! quorum reads did): a read returns the state at its position in the
//! committed order. No proposal is retried; a proposal lost on the way to the
//! leader simply never commits, and the caller's deadline reports an unknown
//! outcome, as `AtomicJournalStore` requires.
//!
//! This is a draft for users to try: membership changes, snapshots/log
//! compaction and ReadIndex/lease reads are not implemented.
use super::cluster::JournalVersion;
use std::collections::{BTreeMap, BTreeSet};

/// splitmix64, used only to randomize election timeouts.
#[derive(Clone, Debug)]
struct Jitter(u64);
impl Jitter {
    fn below(&mut self, bound: u64) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) % bound.max(1)
    }
}

#[derive(Clone, Debug)]
pub struct RaftConfig {
    /// This node's index in `0..members`.
    pub id: usize,
    pub members: usize,
    /// The base election timeout in local clock units; each election waits a
    /// random time in `[election_timeout, 2 * election_timeout)`.
    pub election_timeout: u64,
    pub heartbeat_interval: u64,
    pub pre_vote: bool,
    pub check_quorum: bool,
    /// Seeds the election jitter.
    pub seed: u64,
    /// The most entries one append message carries.
    pub max_entries_per_message: usize,
}
impl RaftConfig {
    pub fn new(id: usize, members: usize) -> Self {
        Self {
            id,
            members,
            election_timeout: 100,
            heartbeat_interval: 10,
            pre_vote: true,
            check_quorum: true,
            seed: id as u64 + 1,
            max_entries_per_message: 32,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RaftCommand {
    /// Starts every term: commits the entries of earlier terms.
    Noop,
    Read {
        id: u64,
        keys: Vec<String>,
    },
    CompareExchange {
        id: u64,
        keys: Vec<String>,
        expected: Vec<JournalVersion>,
        updates: Vec<Option<Vec<String>>>,
    },
}
impl RaftCommand {
    pub fn id(&self) -> Option<u64> {
        match self {
            RaftCommand::Noop => None,
            RaftCommand::Read { id, .. } | RaftCommand::CompareExchange { id, .. } => Some(*id),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaftEntry {
    pub term: u64,
    pub command: RaftCommand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RaftOutcome {
    Read(Vec<JournalVersion>),
    Exchanged(bool),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RaftMessage {
    Vote {
        pre: bool,
        term: u64,
        last_index: u64,
        last_term: u64,
    },
    VoteReply {
        pre: bool,
        term: u64,
        granted: bool,
    },
    Append {
        term: u64,
        prev_index: u64,
        prev_term: u64,
        entries: Vec<RaftEntry>,
        commit: u64,
    },
    /// `index` is the last matching index on success and a hint otherwise.
    AppendReply {
        term: u64,
        success: bool,
        index: u64,
    },
    /// A follower forwards a proposal to the leader it knows.
    Propose {
        command: RaftCommand,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaftEnvelope {
    pub from: usize,
    pub to: usize,
    pub message: RaftMessage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaftRole {
    Follower,
    PreCandidate,
    Candidate,
    Leader,
}

/// What must survive a restart: the term, the vote and the log.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RaftHardState {
    pub term: u64,
    pub voted_for: Option<usize>,
}

/// The replicated journals, as `AtomicJournalStore` exposes them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JournalState {
    /// Bumped by every successful compare-exchange; never reused.
    pub revision: u64,
    pub journals: BTreeMap<String, JournalVersion>,
    /// `(revision, command id)` of every successful compare-exchange.
    pub exchanges: Vec<(u64, u64)>,
}
impl JournalState {
    fn apply(&mut self, command: &RaftCommand) -> Option<RaftOutcome> {
        match command {
            RaftCommand::Noop => None,
            RaftCommand::Read { keys, .. } => Some(RaftOutcome::Read(
                keys.iter()
                    .map(|key| self.journals.get(key).cloned().unwrap_or_default())
                    .collect(),
            )),
            RaftCommand::CompareExchange {
                id,
                keys,
                expected,
                updates,
            } => {
                if keys.len() != expected.len()
                    || keys.len() != updates.len()
                    || keys.iter().zip(expected).any(|(key, version)| {
                        self.journals.get(key).map_or(0, |v| v.generation) != version.generation
                    })
                {
                    return Some(RaftOutcome::Exchanged(false));
                }
                self.revision += 1;
                for (key, update) in keys.iter().zip(updates) {
                    if let Some(entries) = update {
                        self.journals.insert(
                            key.clone(),
                            JournalVersion {
                                generation: self.revision,
                                entries: entries.clone(),
                            },
                        );
                    }
                }
                self.exchanges.push((self.revision, *id));
                Some(RaftOutcome::Exchanged(true))
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct RaftNode {
    config: RaftConfig,
    jitter: Jitter,
    // Persistent.
    hard: RaftHardState,
    /// `log[0]` is a sentinel of term 0.
    log: Vec<RaftEntry>,
    // Volatile.
    role: RaftRole,
    leader: Option<usize>,
    commit: u64,
    applied: u64,
    state: JournalState,
    votes: BTreeSet<usize>,
    next: Vec<u64>,
    matched: Vec<u64>,
    /// Peers heard from since the last quorum check.
    active: BTreeSet<usize>,
    election_due: u64,
    heartbeat_due: u64,
    quorum_due: u64,
    /// When this node last heard from a current leader.
    leader_heard: Option<u64>,
    waiting: BTreeSet<u64>,
    outcomes: BTreeMap<u64, RaftOutcome>,
    outbox: Vec<RaftEnvelope>,
}

impl RaftNode {
    pub fn new(config: RaftConfig, now: u64) -> Self {
        assert!(config.id < config.members && config.members > 0);
        assert!(config.election_timeout > config.heartbeat_interval);
        let jitter = Jitter(config.seed);
        let members = config.members;
        let mut node = Self {
            config,
            jitter,
            hard: RaftHardState::default(),
            log: vec![RaftEntry {
                term: 0,
                command: RaftCommand::Noop,
            }],
            role: RaftRole::Follower,
            leader: None,
            commit: 0,
            applied: 0,
            state: JournalState::default(),
            votes: BTreeSet::new(),
            next: vec![1; members],
            matched: vec![0; members],
            active: BTreeSet::new(),
            election_due: 0,
            heartbeat_due: 0,
            quorum_due: 0,
            leader_heard: None,
            waiting: BTreeSet::new(),
            outcomes: BTreeMap::new(),
            outbox: Vec::new(),
        };
        node.reset_election(now);
        node
    }

    /// A process restart: the hard state and the log survive, everything
    /// else (commit index, applied state, pending outcomes) is rebuilt.
    pub fn restart(&mut self, now: u64) {
        self.role = RaftRole::Follower;
        self.leader = None;
        self.commit = 0;
        self.applied = 0;
        self.state = JournalState::default();
        self.votes.clear();
        self.active.clear();
        self.leader_heard = None;
        self.waiting.clear();
        self.outcomes.clear();
        self.outbox.clear();
        self.reset_election(now);
    }

    pub fn id(&self) -> usize {
        self.config.id
    }
    pub fn role(&self) -> RaftRole {
        self.role
    }
    pub fn term(&self) -> u64 {
        self.hard.term
    }
    pub fn leader(&self) -> Option<usize> {
        self.leader
    }
    pub fn hard_state(&self) -> &RaftHardState {
        &self.hard
    }
    pub fn commit_index(&self) -> u64 {
        self.commit
    }
    pub fn applied_index(&self) -> u64 {
        self.applied
    }
    /// The whole log, sentinel included.
    pub fn log(&self) -> &[RaftEntry] {
        &self.log
    }
    pub fn state(&self) -> &JournalState {
        &self.state
    }
    /// Messages to deliver; persist the hard state and log first.
    pub fn take_messages(&mut self) -> Vec<RaftEnvelope> {
        std::mem::take(&mut self.outbox)
    }
    /// The outcome of a command proposed on this node, once applied here.
    pub fn take_outcome(&mut self, id: u64) -> Option<RaftOutcome> {
        self.outcomes.remove(&id)
    }
    /// Gives up waiting for a command (its caller timed out).
    pub fn forget(&mut self, id: u64) {
        self.waiting.remove(&id);
        self.outcomes.remove(&id);
    }

    /// Proposes a command. Returns false when no leader is known, so the
    /// proposal went nowhere. Even on true the proposal may be lost.
    pub fn propose(&mut self, now: u64, command: RaftCommand) -> bool {
        let _ = now;
        if let Some(id) = command.id() {
            self.waiting.insert(id);
        }
        match (self.role, self.leader) {
            (RaftRole::Leader, _) => {
                self.append_local(command);
                self.broadcast_append();
                true
            }
            (_, Some(leader)) => {
                self.send(leader, RaftMessage::Propose { command });
                true
            }
            _ => false,
        }
    }

    /// Advances timers to the local time `now`. A clock that jumps backward
    /// only delays them.
    pub fn tick(&mut self, now: u64) {
        if self.role == RaftRole::Leader {
            if self.config.check_quorum && now >= self.quorum_due {
                self.quorum_due = now + self.config.election_timeout;
                let heard = self.active.len() + 1;
                self.active.clear();
                if heard < self.quorum() {
                    self.become_follower(now, self.hard.term, None);
                    return;
                }
            }
            if now >= self.heartbeat_due {
                self.heartbeat_due = now + self.config.heartbeat_interval;
                self.broadcast_append();
            }
        } else if now >= self.election_due {
            self.campaign(now, self.config.pre_vote);
        }
    }

    pub fn step(&mut self, now: u64, envelope: RaftEnvelope) {
        let from = envelope.from;
        if from >= self.config.members || from == self.config.id {
            return;
        }
        let term = match &envelope.message {
            RaftMessage::Vote { term, .. }
            | RaftMessage::VoteReply { term, .. }
            | RaftMessage::Append { term, .. }
            | RaftMessage::AppendReply { term, .. } => Some(*term),
            RaftMessage::Propose { .. } => None,
        };
        if let Some(term) = term {
            if term > self.hard.term {
                if matches!(envelope.message, RaftMessage::Vote { .. }) && self.in_lease(now) {
                    // Leader stickiness: ignore disruptive campaigns.
                    return;
                }
                let pre_request = matches!(envelope.message, RaftMessage::Vote { pre: true, .. });
                let pre_grant = matches!(
                    envelope.message,
                    RaftMessage::VoteReply {
                        pre: true,
                        granted: true,
                        ..
                    }
                );
                if !pre_request && !pre_grant {
                    let leader = match envelope.message {
                        RaftMessage::Append { .. } => Some(from),
                        _ => None,
                    };
                    self.become_follower(now, term, leader);
                }
            } else if term < self.hard.term {
                match envelope.message {
                    // Tell a stale leader or candidate about the newer term.
                    RaftMessage::Append { .. } => self.send(
                        from,
                        RaftMessage::AppendReply {
                            term: self.hard.term,
                            success: false,
                            index: 0,
                        },
                    ),
                    RaftMessage::Vote { pre, .. } => self.send(
                        from,
                        RaftMessage::VoteReply {
                            pre,
                            term: self.hard.term,
                            granted: false,
                        },
                    ),
                    _ => {}
                }
                return;
            }
        }
        match envelope.message {
            RaftMessage::Vote {
                pre,
                term,
                last_index,
                last_term,
            } => self.on_vote(now, from, pre, term, last_index, last_term),
            RaftMessage::VoteReply { pre, term, granted } => {
                self.on_vote_reply(now, from, pre, term, granted)
            }
            RaftMessage::Append {
                prev_index,
                prev_term,
                entries,
                commit,
                ..
            } => self.on_append(now, from, prev_index, prev_term, entries, commit),
            RaftMessage::AppendReply { success, index, .. } => {
                self.on_append_reply(from, success, index)
            }
            RaftMessage::Propose { command } => {
                if self.role == RaftRole::Leader {
                    self.append_local(command);
                    self.broadcast_append();
                }
                // Otherwise the proposal is dropped: no proposal is retried.
            }
        }
    }

    fn quorum(&self) -> usize {
        self.config.members / 2 + 1
    }
    fn last_index(&self) -> u64 {
        self.log.len() as u64 - 1
    }
    fn last_term(&self) -> u64 {
        self.log.last().map_or(0, |entry| entry.term)
    }
    fn in_lease(&self, now: u64) -> bool {
        self.config.check_quorum
            && match self.role {
                RaftRole::Leader => true,
                _ => {
                    self.leader.is_some()
                        && self
                            .leader_heard
                            .is_some_and(|heard| now < heard + self.config.election_timeout)
                }
            }
    }
    fn send(&mut self, to: usize, message: RaftMessage) {
        self.outbox.push(RaftEnvelope {
            from: self.config.id,
            to,
            message,
        });
    }
    fn reset_election(&mut self, now: u64) {
        self.election_due =
            now + self.config.election_timeout + self.jitter.below(self.config.election_timeout);
    }
    fn become_follower(&mut self, now: u64, term: u64, leader: Option<usize>) {
        if term > self.hard.term {
            self.hard.term = term;
            self.hard.voted_for = None;
        }
        self.role = RaftRole::Follower;
        self.leader = leader;
        self.votes.clear();
        if leader.is_some() {
            self.leader_heard = Some(now);
        }
        self.reset_election(now);
    }
    fn campaign(&mut self, now: u64, pre: bool) {
        self.reset_election(now);
        self.votes.clear();
        self.votes.insert(self.config.id);
        let term = if pre {
            self.role = RaftRole::PreCandidate;
            self.hard.term + 1
        } else {
            self.role = RaftRole::Candidate;
            self.hard.term += 1;
            self.hard.voted_for = Some(self.config.id);
            self.hard.term
        };
        self.leader = None;
        if self.votes.len() >= self.quorum() {
            self.win(now, pre);
            return;
        }
        let (last_index, last_term) = (self.last_index(), self.last_term());
        for peer in 0..self.config.members {
            if peer != self.config.id {
                self.send(
                    peer,
                    RaftMessage::Vote {
                        pre,
                        term,
                        last_index,
                        last_term,
                    },
                );
            }
        }
    }
    fn win(&mut self, now: u64, pre: bool) {
        if pre {
            self.campaign(now, false);
            return;
        }
        self.role = RaftRole::Leader;
        self.leader = Some(self.config.id);
        self.votes.clear();
        self.active.clear();
        let next = self.last_index() + 1;
        self.next.iter_mut().for_each(|value| *value = next);
        self.matched.iter_mut().for_each(|value| *value = 0);
        self.quorum_due = now + self.config.election_timeout;
        self.heartbeat_due = now + self.config.heartbeat_interval;
        self.append_local(RaftCommand::Noop);
        self.broadcast_append();
    }
    fn on_vote(
        &mut self,
        now: u64,
        from: usize,
        pre: bool,
        term: u64,
        last_index: u64,
        last_term: u64,
    ) {
        let up_to_date = last_term > self.last_term()
            || (last_term == self.last_term() && last_index >= self.last_index());
        let granted = if pre {
            // A pre-vote never changes this node's state.
            term > self.hard.term && up_to_date && !self.in_lease(now)
        } else {
            term == self.hard.term
                && up_to_date
                && self.hard.voted_for.is_none_or(|voted| voted == from)
        };
        if granted && !pre {
            self.hard.voted_for = Some(from);
            self.reset_election(now);
        }
        let term = if pre && granted { term } else { self.hard.term };
        self.send(from, RaftMessage::VoteReply { pre, term, granted });
    }
    fn on_vote_reply(&mut self, now: u64, from: usize, pre: bool, term: u64, granted: bool) {
        let expected = match (self.role, pre) {
            (RaftRole::PreCandidate, true) => self.hard.term + 1,
            (RaftRole::Candidate, false) => self.hard.term,
            _ => return,
        };
        if term != expected || !granted {
            return;
        }
        self.votes.insert(from);
        if self.votes.len() >= self.quorum() {
            self.win(now, pre);
        }
    }
    fn on_append(
        &mut self,
        now: u64,
        from: usize,
        prev_index: u64,
        prev_term: u64,
        entries: Vec<RaftEntry>,
        commit: u64,
    ) {
        // The term equals ours here; a candidate of this term steps down.
        if self.role != RaftRole::Follower || self.leader != Some(from) {
            self.role = RaftRole::Follower;
            self.leader = Some(from);
            self.votes.clear();
        }
        self.leader_heard = Some(now);
        self.reset_election(now);
        if prev_index > self.last_index() || self.log[prev_index as usize].term != prev_term {
            let hint = (prev_index.saturating_sub(1)).min(self.last_index());
            self.send(
                from,
                RaftMessage::AppendReply {
                    term: self.hard.term,
                    success: false,
                    index: hint,
                },
            );
            return;
        }
        let mut index = prev_index;
        for entry in entries {
            index += 1;
            if index <= self.last_index() {
                if self.log[index as usize].term == entry.term {
                    continue;
                }
                // A conflicting suffix was never committed: drop it.
                assert!(
                    index > self.commit,
                    "a committed entry would be overwritten"
                );
                self.log.truncate(index as usize);
            }
            self.log.push(entry);
        }
        if commit > self.commit {
            self.commit = commit.min(index);
            self.apply();
        }
        self.send(
            from,
            RaftMessage::AppendReply {
                term: self.hard.term,
                success: true,
                index,
            },
        );
    }
    fn on_append_reply(&mut self, from: usize, success: bool, index: u64) {
        if self.role != RaftRole::Leader {
            return;
        }
        self.active.insert(from);
        if success {
            if index > self.matched[from] {
                self.matched[from] = index;
            }
            self.next[from] = self.next[from].max(index + 1);
            self.advance_commit();
            if self.next[from] <= self.last_index() {
                self.send_append(from);
            }
        } else {
            self.next[from] = (index + 1).min(self.next[from].saturating_sub(1)).max(1);
            self.send_append(from);
        }
    }
    fn append_local(&mut self, command: RaftCommand) {
        let term = self.hard.term;
        self.log.push(RaftEntry { term, command });
        let id = self.config.id;
        self.matched[id] = self.last_index();
        self.next[id] = self.last_index() + 1;
        self.advance_commit();
    }
    fn broadcast_append(&mut self) {
        for peer in 0..self.config.members {
            if peer != self.config.id {
                self.send_append(peer);
            }
        }
    }
    fn send_append(&mut self, peer: usize) {
        let prev_index = self.next[peer] - 1;
        let prev_term = self.log[prev_index as usize].term;
        let end =
            (self.log.len()).min(prev_index as usize + 1 + self.config.max_entries_per_message);
        let entries = self.log[prev_index as usize + 1..end].to_vec();
        let message = RaftMessage::Append {
            term: self.hard.term,
            prev_index,
            prev_term,
            entries,
            commit: self.commit,
        };
        self.send(peer, message);
    }
    fn advance_commit(&mut self) {
        let mut matched = self.matched.clone();
        matched.sort_unstable_by(|a, b| b.cmp(a));
        let candidate = matched[self.quorum() - 1];
        // Only entries of the current term commit by counting replicas.
        if candidate > self.commit && self.log[candidate as usize].term == self.hard.term {
            self.commit = candidate;
            self.apply();
        }
    }
    fn apply(&mut self) {
        while self.applied < self.commit {
            self.applied += 1;
            let command = &self.log[self.applied as usize].command;
            if let Some(outcome) = self.state.apply(command) {
                let id = command.id().expect("only commands with ids have outcomes");
                if self.waiting.remove(&id) {
                    self.outcomes.insert(id, outcome);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deliver(nodes: &mut [RaftNode], now: u64) {
        loop {
            let messages: Vec<RaftEnvelope> = nodes
                .iter_mut()
                .flat_map(|node| node.take_messages())
                .collect();
            if messages.is_empty() {
                return;
            }
            for message in messages {
                let to = message.to;
                nodes[to].step(now, message);
            }
        }
    }

    fn cluster(pre_vote: bool, check_quorum: bool) -> Vec<RaftNode> {
        (0..3)
            .map(|id| {
                let mut config = RaftConfig::new(id, 3);
                config.pre_vote = pre_vote;
                config.check_quorum = check_quorum;
                RaftNode::new(config, 0)
            })
            .collect()
    }

    #[test]
    fn a_single_member_elects_itself_and_commits() {
        let mut node = RaftNode::new(RaftConfig::new(0, 1), 0);
        node.tick(1000);
        assert_eq!(node.role(), RaftRole::Leader);
        assert!(node.propose(
            1000,
            RaftCommand::Read {
                id: 7,
                keys: vec!["a".into()]
            }
        ));
        assert_eq!(
            node.take_outcome(7),
            Some(RaftOutcome::Read(vec![JournalVersion::default()]))
        );
    }

    #[test]
    fn a_leader_replicates_compare_exchange_to_every_member() {
        let mut nodes = cluster(true, true);
        nodes[0].tick(1000);
        deliver(&mut nodes, 1000);
        assert_eq!(nodes[0].role(), RaftRole::Leader);
        let exchange = RaftCommand::CompareExchange {
            id: 1,
            keys: vec!["a".into()],
            expected: vec![JournalVersion::default()],
            updates: vec![Some(vec!["x".into()])],
        };
        assert!(nodes[1].propose(1001, exchange.clone()));
        deliver(&mut nodes, 1001);
        assert_eq!(nodes[0].commit_index(), 2);
        // Followers learn the commit index from the next heartbeat.
        assert_eq!(nodes[1].take_outcome(1), None);
        nodes[0].tick(1011);
        deliver(&mut nodes, 1011);
        assert_eq!(nodes[1].take_outcome(1), Some(RaftOutcome::Exchanged(true)));
        for node in &nodes {
            assert_eq!(node.state().journals["a"].entries, vec!["x".to_string()]);
        }
        // The same compare-exchange again sees a newer generation.
        let RaftCommand::CompareExchange {
            keys,
            expected,
            updates,
            ..
        } = exchange
        else {
            unreachable!()
        };
        assert!(nodes[2].propose(
            1012,
            RaftCommand::CompareExchange {
                id: 2,
                keys,
                expected,
                updates
            }
        ));
        deliver(&mut nodes, 1012);
        nodes[0].tick(1022);
        deliver(&mut nodes, 1022);
        assert_eq!(
            nodes[2].take_outcome(2),
            Some(RaftOutcome::Exchanged(false))
        );
        assert_eq!(nodes[0].state().exchanges, vec![(1, 1)]);
    }

    #[test]
    fn a_member_inside_a_leader_lease_ignores_a_disruptive_campaign() {
        let mut nodes = cluster(true, true);
        nodes[0].tick(1000);
        deliver(&mut nodes, 1000);
        // Node 2 cannot hear anything, times out and campaigns.
        nodes[2].tick(5000);
        let campaign = nodes[2].take_messages();
        assert!(campaign
            .iter()
            .all(|m| matches!(m.message, RaftMessage::Vote { pre: true, .. })));
        for message in campaign {
            let to = message.to;
            nodes[to].step(1005, message);
        }
        // Neither member replies or raises its term.
        assert!(nodes[0].take_messages().is_empty() && nodes[1].take_messages().is_empty());
        assert_eq!((nodes[0].term(), nodes[0].role()), (1, RaftRole::Leader));
        assert_eq!(nodes[2].term(), 1);
    }

    #[test]
    fn a_leader_that_hears_no_majority_steps_down_under_check_quorum() {
        let mut nodes = cluster(true, true);
        nodes[0].tick(1000);
        deliver(&mut nodes, 1000);
        nodes[0].tick(1100);
        assert_eq!(nodes[0].role(), RaftRole::Leader);
        nodes[0].take_messages();
        nodes[0].tick(1200);
        assert_eq!(nodes[0].role(), RaftRole::Follower);
    }

    #[test]
    fn a_restarted_member_rebuilds_its_state_from_the_log() {
        let mut nodes = cluster(false, false);
        nodes[0].tick(1000);
        deliver(&mut nodes, 1000);
        assert_eq!(nodes[0].role(), RaftRole::Leader);
        nodes[0].propose(
            1000,
            RaftCommand::CompareExchange {
                id: 1,
                keys: vec!["a".into()],
                expected: vec![JournalVersion::default()],
                updates: vec![Some(vec!["x".into()])],
            },
        );
        deliver(&mut nodes, 1000);
        nodes[1].restart(1001);
        assert_eq!(nodes[1].state(), &JournalState::default());
        nodes[0].tick(1011);
        deliver(&mut nodes, 1011);
        assert_eq!(nodes[1].state(), nodes[0].state());
        assert_eq!(nodes[1].hard_state().term, 1);
    }
}
