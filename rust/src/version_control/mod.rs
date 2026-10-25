//! Optional version-control layer for the Rust link-cli.
//!
//! Mirrors the C# `VersionControlDecorator` in
//! `csharp/Foundation.Data.Doublets.Cli.Library/VersionControlDecorator.cs`.
//!
//! Sits above the [`TransactionsDecorator`]
//! and adds *time travel* ([`checkout`](VersionControlDecorator::checkout)),
//! *branching* ([`branch`](VersionControlDecorator::branch),
//! [`switch_branch`](VersionControlDecorator::switch_branch)), and
//! *tagging* ([`tag`](VersionControlDecorator::tag)) over the transitions
//! log. Optional — when not instantiated the underlying transactions
//! decorator behaves identically (R17).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::link::Link;
use crate::link_storage::ChangeObserver;
use crate::named_type_links::NamedTypeLinks;
use crate::named_types::NamedTypesDecorator;

mod publication;
use crate::transactions::{TransactionHandle, TransactionsDecorator};
use publication::{PublicationIntent, PUBLICATION_PREFIX};

/// Default name of the initial branch (analogous to git's `main`).
pub const DEFAULT_BRANCH_NAME: &str = "main";

const BRANCH_PREFIX: &str = "__vc:branch:";
const TAG_PREFIX: &str = "__vc:tag:";
const CURRENT_PREFIX: &str = "__vc:current=";
const APPLIED_PREFIX: &str = "__vc:applied=";
const TRANSITION_PREFIX: &str = "__vc:trans:";

/// Metadata describing one branch in the version-control DAG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInfo {
    pub name: String,
    pub parent: Option<String>,
    pub fork_seq: i64,
    pub head: i64,
}

impl BranchInfo {
    pub fn new(name: String, parent: Option<String>, fork_seq: i64, head: i64) -> Self {
        Self {
            name,
            parent,
            fork_seq,
            head,
        }
    }
}

/// Decorator that adds *time travel*, *branching*, and *tagging* over the
/// transitions log produced by a [`TransactionsDecorator`].
pub type VersionControlDecorator = GenericVersionControlDecorator<NamedTypesDecorator>;

/// Version control over a replaceable metadata store using the common links API.
/// The default alias keeps the existing mapped metadata store and public API.
pub struct GenericVersionControlDecorator<B: NamedTypeLinks> {
    transactions: TransactionsDecorator,
    branches_store: B,
    branches: HashMap<String, BranchInfo>,
    tags: BTreeMap<String, i64>,
    transition_branches: BTreeMap<i64, String>,
    branch_links: HashMap<String, u32>,
    tag_links: HashMap<String, u32>,
    current_branch_link: u32,
    applied_link: u32,
    current_branch: String,
    current_applied: i64,
    active_transaction: Option<VersionControlTransactionState>,
    trace: bool,
}

#[derive(Debug, Clone)]
struct VersionControlTransactionState {
    id: u128,
    branch_name: String,
    before_sequence: i64,
    after_sequence: Option<i64>,
    intent_link: u32,
}

impl<B: NamedTypeLinks> GenericVersionControlDecorator<B> {
    pub fn new(
        transactions: TransactionsDecorator,
        branches_store: B,
        trace: bool,
    ) -> Result<Self> {
        let mut decorator = Self {
            transactions,
            branches_store,
            branches: HashMap::new(),
            tags: BTreeMap::new(),
            transition_branches: BTreeMap::new(),
            branch_links: HashMap::new(),
            tag_links: HashMap::new(),
            current_branch_link: 0,
            applied_link: 0,
            current_branch: DEFAULT_BRANCH_NAME.to_string(),
            current_applied: 0,
            active_transaction: None,
            trace,
        };
        decorator.recover()?;
        decorator.ensure_default_branch()?;
        Ok(decorator)
    }

    /// Conventional sidecar filename for the version-control store.
    pub fn make_version_control_database_filename<P: AsRef<Path>>(p: P) -> PathBuf {
        let path = p.as_ref();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let name = format!("{stem}.versioncontrol.links");
        match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(name),
            _ => PathBuf::from(name),
        }
    }

    pub fn current_branch(&self) -> &str {
        &self.current_branch
    }

    pub fn current_sequence(&self) -> i64 {
        self.current_applied
    }

    pub fn list_branches(&self) -> Vec<BranchInfo> {
        let mut branches: Vec<BranchInfo> = self.branches.values().cloned().collect();
        branches.sort_by(|a, b| a.name.cmp(&b.name));
        branches
    }

    pub fn list_tags(&self) -> BTreeMap<String, i64> {
        self.tags.clone()
    }

    pub fn try_get_tag(&self, name: &str) -> Option<i64> {
        self.tags.get(name).copied()
    }

    pub fn save(&mut self) -> Result<()> {
        if self.has_pending_publication() {
            self.commit()?;
        }
        self.transactions.save()?;
        self.branches_store.save()?;
        Ok(())
    }

    pub fn transactions(&self) -> &TransactionsDecorator {
        &self.transactions
    }

    pub fn transactions_mut(&mut self) -> &mut TransactionsDecorator {
        &mut self.transactions
    }

    pub fn branches_store(&self) -> &B {
        &self.branches_store
    }

    pub fn begin_transaction(&mut self) -> Result<TransactionHandle> {
        if self.active_transaction.is_some() {
            bail!("Nested version-control transactions are not supported.");
        }
        let before_sequence = self.transactions.last_logged_sequence();
        let branch_name = self.current_branch.clone();
        let handle = self.transactions.begin_transaction()?;
        self.active_transaction = Some(VersionControlTransactionState {
            id: handle.id,
            branch_name,
            before_sequence,
            after_sequence: None,
            intent_link: 0,
        });
        Ok(handle)
    }

    pub fn commit(&mut self) -> Result<()> {
        let state = self
            .active_transaction
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No version-control transaction is open."))?;
        if self.transactions.is_transaction_rolled_back(state.id) {
            return self.finish_aborted_publication(&state);
        }
        let state = self.prepare_publication()?;
        self.transactions.commit()?;
        self.attribute_new_transitions_for_branch(
            state.before_sequence,
            state.after_sequence.expect("publication range is frozen"),
            &state.branch_name,
        )?;
        self.branches_store.save()?;
        self.clear_publication_intent(state.intent_link)?;
        self.active_transaction = None;
        Ok(())
    }

    pub fn rollback(&mut self) -> Result<()> {
        if self.has_pending_commit() {
            // An uncertain/durable decision must finish before publication.
            return self.commit();
        }
        let state = self
            .active_transaction
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No version-control transaction is open."))?;
        let outcome = self.transactions.rollback();
        if outcome.is_ok() || self.transactions.is_transaction_rolled_back(state.id) {
            self.finish_aborted_publication(&state)?;
        }
        Ok(outcome?)
    }

    // -- Write API (attribute new transitions to the current branch) ----

    pub fn create(&mut self, source: u32, target: u32) -> Result<u32> {
        self.run_version_write(|transactions| transactions.create(source, target))
    }

    pub fn update(&mut self, id: u32, source: u32, target: u32) -> Result<Link> {
        self.update_observed(id, source, target, &mut |_, _| {})
    }

    /// [`Self::update`], reporting every change the decorator stack made —
    /// the deletion of a link merged into its duplicate included.
    pub fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: ChangeObserver<'_>,
    ) -> Result<Link> {
        self.run_version_write(|transactions| {
            transactions.update_observed(id, source, target, observer)
        })
    }

    pub fn delete(&mut self, id: u32) -> Result<Link> {
        self.delete_observed(id, &mut |_, _| {})
    }

    /// [`Self::delete`], reporting every change the decorator stack made —
    /// cascaded deletions of usages included.
    pub fn delete_observed(&mut self, id: u32, observer: ChangeObserver<'_>) -> Result<Link> {
        self.run_version_write(|transactions| transactions.delete_observed(id, observer))
    }

    pub fn create_and_update(&mut self, source: u32, target: u32) -> Result<u32> {
        self.run_version_write(|transactions| transactions.create_and_update(source, target))
    }

    pub fn exists(&self, id: u32) -> bool {
        self.transactions.exists(id)
    }

    pub fn get(&self, id: u32) -> Option<&Link> {
        self.transactions.get(id)
    }

    pub fn all(&self) -> Vec<&Link> {
        self.transactions.all()
    }

    pub fn search(&self, source: u32, target: u32) -> Option<u32> {
        self.transactions.search(source, target)
    }

    pub fn get_or_create(&mut self, source: u32, target: u32) -> Result<u32> {
        self.ensure_write_allowed()?;
        if let Some(existing) = self.transactions.search(source, target) {
            return Ok(existing);
        }
        self.create(source, target)
    }

    pub fn ensure_created(&mut self, id: u32) -> u32 {
        self.try_ensure_created(id)
            .expect("VersionControlDecorator::ensure_created failed")
    }

    /// Fallible explicit-address allocation, attributed like other writes.
    pub fn try_ensure_created(&mut self, id: u32) -> Result<u32> {
        self.ensure_write_allowed()?;
        // Existing and zero addresses remain read-only no-ops: do not open a
        // transaction or write a decision marker for them.
        if id == 0 || self.transactions.exists(id) {
            return Ok(id);
        }
        self.run_version_write(|transactions| transactions.ensure_created(id))
    }

    fn run_version_write<T>(
        &mut self,
        write: impl FnOnce(&mut TransactionsDecorator) -> std::result::Result<T, crate::LinkError>,
    ) -> Result<T> {
        self.ensure_write_allowed()?;
        let automatic = self.active_transaction.is_none();
        if automatic {
            self.begin_transaction()?;
        }
        let result = match write(&mut self.transactions) {
            Ok(result) => result,
            Err(error) => {
                // Pre-commit failures still undo automatic effects. An undo
                // failure leaves the facade owner available for retry.
                if automatic {
                    self.rollback()?;
                }
                return Err(error.into());
            }
        };
        // Never undo a commit error: its decision may already be durable.
        if automatic {
            self.commit()?;
        }
        Ok(result)
    }

    fn attribute_new_transitions_for_branch(
        &mut self,
        before_seq: i64,
        after_seq: i64,
        branch_name: &str,
    ) -> Result<()> {
        if after_seq <= before_seq {
            return Ok(());
        }
        for s in (before_seq + 1)..=after_seq {
            self.transition_branches.insert(s, branch_name.to_string());
            let marker = format!("{TRANSITION_PREFIX}{s}:branch={branch_name}");
            self.write_immutable_marker(&marker)?;
        }
        if let Some(info) = self.branches.get(branch_name).cloned() {
            let updated = BranchInfo {
                head: after_seq,
                ..info
            };
            self.branches
                .insert(branch_name.to_string(), updated.clone());
            self.update_branch_link(&updated)?;
        }
        if self.current_branch == branch_name {
            self.current_applied = after_seq;
            self.set_applied(after_seq)?;
        }
        Ok(())
    }

    // -- Branching ----------------------------------------------------

    pub fn branch(&mut self, name: &str, from: Option<i64>) -> Result<()> {
        self.ensure_no_open_transaction("branch")?;
        if name.trim().is_empty() {
            bail!("Branch name must not be empty.");
        }
        if self.branches.contains_key(name) {
            bail!("Branch '{name}' already exists.");
        }
        let parent = self.current_branch.clone();
        let fork_seq = from.unwrap_or(self.current_applied);
        if fork_seq < 0 {
            bail!("Fork point cannot be negative.");
        }
        if fork_seq > 0 {
            let path = self.build_branch_seqs(&parent);
            if !path.contains(&fork_seq) {
                bail!("Fork point {fork_seq} is not reachable on branch '{parent}'.",);
            }
        }
        self.create_branch(name, Some(parent), fork_seq, fork_seq)?;
        self.trace(&format!(
            "Created branch '{name}' from '{}' at seq {fork_seq}.",
            self.current_branch
        ));
        Ok(())
    }

    pub fn switch_branch(&mut self, name: &str) -> Result<()> {
        self.ensure_no_open_transaction("switch_branch")?;
        if !self.branches.contains_key(name) {
            bail!("Unknown branch '{name}'.");
        }
        let target_path = self.build_branch_seqs(name);
        self.apply_diff_to(target_path, name)?;
        self.trace(&format!(
            "Switched to branch '{name}' at seq {}.",
            self.current_applied
        ));
        Ok(())
    }

    pub fn checkout(&mut self, sequence: i64) -> Result<()> {
        self.ensure_no_open_transaction("checkout")?;
        if sequence < 0 {
            bail!("Sequence must be non-negative.");
        }
        let current = self.current_branch.clone();
        let path = self.build_branch_seqs(&current);
        if sequence > 0 && !path.contains(&sequence) {
            bail!("Sequence {sequence} is not reachable on branch '{current}'.",);
        }
        let target_path: Vec<i64> = path.iter().copied().filter(|s| *s <= sequence).collect();
        self.apply_diff_to(target_path, &current)?;
        self.trace(&format!(
            "Checked out seq {sequence} on branch '{current}'.",
        ));
        Ok(())
    }

    pub fn tag(&mut self, name: &str, sequence: Option<i64>) -> Result<()> {
        self.ensure_no_open_transaction("tag")?;
        if name.trim().is_empty() {
            bail!("Tag name must not be empty.");
        }
        let seq = sequence.unwrap_or(self.current_applied);
        if seq < 0 {
            bail!("Tag sequence must be non-negative.");
        }
        self.tags.insert(name.to_string(), seq);
        self.update_tag_link(name, seq)?;
        self.trace(&format!("Created tag '{name}' at seq {seq}.",));
        Ok(())
    }

    // -- Path / diff helpers -----------------------------------------

    fn apply_diff_to(&mut self, target_path: Vec<i64>, new_branch: &str) -> Result<()> {
        let current_branch_name = self.current_branch.clone();
        let current_path: Vec<i64> = self
            .build_branch_seqs(&current_branch_name)
            .into_iter()
            .filter(|s| *s <= self.current_applied)
            .collect();

        let mut common = 0usize;
        let max_common = current_path.len().min(target_path.len());
        while common < max_common && current_path[common] == target_path[common] {
            common += 1;
        }

        if common < current_path.len() || common < target_path.len() {
            // Validate both suffixes before undoing even one row. Retention
            // can remove a later-needed redo while the current undo survives.
            let available: HashMap<_, _> = self
                .transactions
                .log()
                .into_iter()
                .map(|transition| (transition.sequence, transition))
                .collect();
            let required = |sequence: &i64| {
                available.get(sequence).copied().ok_or_else(|| {
                    anyhow::anyhow!("Transition {sequence} is no longer retained; checkout requires complete changed history.")
                })
            };
            let undo: Vec<_> = current_path[common..]
                .iter()
                .rev()
                .map(required)
                .collect::<Result<_>>()?;
            let redo: Vec<_> = target_path[common..]
                .iter()
                .map(required)
                .collect::<Result<_>>()?;
            for transition in undo {
                self.transactions.revert_transition(&transition)?;
            }
            for transition in redo {
                self.transactions.apply_transition(&transition)?;
            }
        }

        if new_branch != self.current_branch {
            self.current_branch = new_branch.to_string();
            self.set_current_branch(new_branch)?;
        }
        self.current_applied = target_path.last().copied().unwrap_or(0);
        self.set_applied(self.current_applied)?;
        Ok(())
    }

    fn ensure_no_open_transaction(&self, operation: &str) -> Result<()> {
        if self.active_transaction.is_some() || self.transactions.has_open_transaction() {
            bail!("{operation} is not allowed while a version-control transaction is open.");
        }
        Ok(())
    }

    fn has_pending_commit(&self) -> bool {
        self.active_transaction
            .as_ref()
            .is_some_and(|state| self.transactions.must_finish_commit(state.id))
    }

    fn has_pending_publication(&self) -> bool {
        self.active_transaction
            .as_ref()
            .is_some_and(|state| state.after_sequence.is_some())
            || self.has_pending_commit()
    }

    pub(crate) fn ensure_write_allowed(&self) -> Result<()> {
        if self.has_pending_publication() {
            bail!(
                "Version publication is incomplete; retry commit, rollback or save before writing."
            );
        }
        self.transactions.ensure_write_allowed()?;
        Ok(())
    }

    fn build_branch_seqs(&self, branch_name: &str) -> Vec<i64> {
        let mut visited: HashSet<String> = HashSet::new();
        self.build_branch_seqs_inner(branch_name, &mut visited)
    }

    fn build_branch_seqs_inner(
        &self,
        branch_name: &str,
        visited: &mut HashSet<String>,
    ) -> Vec<i64> {
        let info = match self.branches.get(branch_name) {
            Some(info) => info,
            None => return Vec::new(),
        };
        if !visited.insert(branch_name.to_string()) {
            return Vec::new();
        }
        let mut seqs = Vec::new();
        if let Some(parent_name) = info.parent.as_deref() {
            if self.branches.contains_key(parent_name) {
                let mut parent_seqs = self.build_branch_seqs_inner(parent_name, visited);
                parent_seqs.retain(|s| *s <= info.fork_seq);
                seqs.extend(parent_seqs);
            }
        }
        let mut own: Vec<i64> = self
            .transition_branches
            .iter()
            .filter(|(s, b)| b.as_str() == branch_name && **s <= info.head)
            .map(|(s, _)| *s)
            .collect();
        own.sort();
        seqs.extend(own);
        seqs
    }

    // -- Persistence helpers -----------------------------------------

    fn ensure_default_branch(&mut self) -> Result<()> {
        if !self.branches.contains_key(DEFAULT_BRANCH_NAME) {
            let mut existing = 0;
            // Aborted and recovered-pending records have sequence numbers,
            // but only a known committed decision makes them versions.
            for transition in self.transactions.log() {
                if !self
                    .transactions
                    .is_transaction_committed(transition.transaction_id)
                {
                    continue;
                }
                let s = transition.sequence;
                existing = existing.max(s);
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    self.transition_branches.entry(s)
                {
                    entry.insert(DEFAULT_BRANCH_NAME.to_string());
                    let marker = format!("{TRANSITION_PREFIX}{s}:branch={DEFAULT_BRANCH_NAME}");
                    self.write_immutable_marker(&marker)?;
                }
            }
            self.create_branch(DEFAULT_BRANCH_NAME, None, 0, existing)?;
            self.current_branch = DEFAULT_BRANCH_NAME.to_string();
            self.current_applied = existing;
            self.set_current_branch(DEFAULT_BRANCH_NAME)?;
            self.set_applied(existing)?;
        } else if self.current_branch_link == 0 {
            let branch = self.current_branch.clone();
            self.set_current_branch(&branch)?;
        }
        Ok(())
    }

    fn create_branch(
        &mut self,
        name: &str,
        parent: Option<String>,
        fork_seq: i64,
        head: i64,
    ) -> Result<()> {
        let info = BranchInfo::new(name.to_string(), parent, fork_seq, head);
        self.branches.insert(name.to_string(), info.clone());
        self.update_branch_link(&info)?;
        Ok(())
    }

    fn update_branch_link(&mut self, info: &BranchInfo) -> Result<()> {
        let marker = encode_branch_marker(info);
        let link = match self.branch_links.get(&info.name).copied() {
            Some(link) => link,
            None => {
                let new_link = self.branches_store.try_create(0, 0)?;
                self.branch_links.insert(info.name.clone(), new_link);
                new_link
            }
        };
        self.branches_store.set_name(link, &marker)?;
        Ok(())
    }

    fn update_tag_link(&mut self, name: &str, seq: i64) -> Result<()> {
        let marker = format!("{TAG_PREFIX}{name}={seq}");
        let link = match self.tag_links.get(name).copied() {
            Some(link) => link,
            None => {
                let new_link = self.branches_store.try_create(0, 0)?;
                self.tag_links.insert(name.to_string(), new_link);
                new_link
            }
        };
        self.branches_store.set_name(link, &marker)?;
        Ok(())
    }

    fn set_current_branch(&mut self, name: &str) -> Result<()> {
        self.current_branch = name.to_string();
        if self.current_branch_link == 0 {
            self.current_branch_link = self.branches_store.try_create(0, 0)?;
        }
        let link = self.current_branch_link;
        let marker = format!("{CURRENT_PREFIX}{name}");
        self.branches_store.set_name(link, &marker)?;
        Ok(())
    }

    fn set_applied(&mut self, seq: i64) -> Result<()> {
        if self.applied_link == 0 {
            self.applied_link = self.branches_store.try_create(0, 0)?;
        }
        let link = self.applied_link;
        let marker = format!("{APPLIED_PREFIX}{seq}");
        self.branches_store.set_name(link, &marker)?;
        Ok(())
    }

    fn write_immutable_marker(&mut self, name: &str) -> Result<()> {
        if self.branches_store.get_by_name(name)?.is_some() {
            return Ok(());
        }
        let link = self.branches_store.try_create(0, 0)?;
        self.branches_store.set_name(link, name)?;
        Ok(())
    }

    pub fn recover(&mut self) -> Result<()> {
        self.ensure_no_open_transaction("recover")?;
        let uncommitted: HashSet<_> = self
            .transactions
            .log()
            .into_iter()
            .filter(|t| !self.transactions.is_transaction_committed(t.transaction_id))
            .map(|t| t.sequence)
            .collect();
        self.branches.clear();
        self.tags.clear();
        self.transition_branches.clear();
        self.branch_links.clear();
        self.tag_links.clear();
        self.current_branch = DEFAULT_BRANCH_NAME.to_string();
        self.current_branch_link = 0;
        self.applied_link = 0;
        self.current_applied = 0;

        let mut pending = Vec::new();
        let links = self.branches_store.all_links();
        for link in &links {
            let name = match self.branches_store.get_name(link.index)? {
                Some(value) => value,
                None => continue,
            };
            if name.starts_with(PUBLICATION_PREFIX) {
                pending.push((
                    link.index,
                    PublicationIntent::decode(&name)
                        .ok_or_else(|| anyhow::anyhow!("Invalid version publication intent."))?,
                ));
            } else if name.starts_with(BRANCH_PREFIX) {
                if let Some(info) = try_decode_branch_marker(&name) {
                    self.branches.insert(info.name.clone(), info.clone());
                    self.branch_links.insert(info.name.clone(), link.index);
                }
            } else if let Some(rest) = name.strip_prefix(CURRENT_PREFIX) {
                self.current_branch = rest.to_string();
                self.current_branch_link = link.index;
            } else if let Some(rest) = name.strip_prefix(APPLIED_PREFIX) {
                if let Ok(seq) = rest.parse::<i64>() {
                    self.current_applied = seq;
                    self.applied_link = link.index;
                }
            } else if let Some(rest) = name.strip_prefix(TAG_PREFIX) {
                if let Some(eq) = rest.rfind('=').filter(|&index| index > 0) {
                    let tag_name = &rest[..eq];
                    if let Ok(tag_seq) = rest[eq + 1..].parse::<i64>() {
                        self.tags.insert(tag_name.to_string(), tag_seq);
                        self.tag_links.insert(tag_name.to_string(), link.index);
                    }
                }
            } else if let Some(rest) = name.strip_prefix(TRANSITION_PREFIX) {
                if let Some(colon) = rest.find(":branch=") {
                    if let Ok(seq) = rest[..colon].parse::<i64>() {
                        if uncommitted.contains(&seq) {
                            bail!("Version metadata references uncommitted transition {seq}.");
                        }
                        let branch_name = &rest[colon + ":branch=".len()..];
                        self.transition_branches
                            .insert(seq, branch_name.to_string());
                    }
                }
            }
        }
        self.recover_publications(pending)
    }

    fn trace(&self, message: &str) {
        if self.trace {
            crate::diagnostics::stderr!("[VersionControl] {message}");
        }
    }
}

fn encode_branch_marker(info: &BranchInfo) -> String {
    let parent = info
        .parent
        .as_deref()
        .map(encode_version_name)
        .unwrap_or_else(|| "-".to_string());
    format!(
        "{BRANCH_PREFIX}2|{name}|{parent}|{fork}|{head}",
        name = encode_version_name(&info.name),
        fork = info.fork_seq,
        head = info.head,
    )
}

// Four hexadecimal digits per UTF-16 code unit match the C# sidecar format.
fn encode_version_name(name: &str) -> String {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let mut text = String::new();
    for unit in name.encode_utf16() {
        for shift in [12, 8, 4, 0] {
            text.push(DIGITS[((unit >> shift) & 15) as usize] as char);
        }
    }
    text
}

fn decode_version_name(text: &str) -> Option<String> {
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let mut units = Vec::with_capacity(text.len() / 4);
    for digits in text.as_bytes().as_chunks::<4>().0 {
        let mut unit = 0u16;
        for &digit in digits {
            unit = (unit << 4)
                | match digit {
                    b'0'..=b'9' => (digit - b'0') as u16,
                    b'a'..=b'f' => (digit - b'a' + 10) as u16,
                    b'A'..=b'F' => (digit - b'A' + 10) as u16,
                    _ => return None,
                };
        }
        units.push(unit);
    }
    String::from_utf16(&units).ok()
}

fn try_decode_branch_marker(text: &str) -> Option<BranchInfo> {
    let rest = text.strip_prefix(BRANCH_PREFIX)?;
    if rest.starts_with("2|") {
        let fields: Vec<_> = rest.split('|').collect();
        if fields.len() == 5 {
            let decoded = (|| {
                Some(BranchInfo::new(
                    decode_version_name(fields[1])?,
                    if fields[2] == "-" {
                        None
                    } else {
                        Some(decode_version_name(fields[2])?)
                    },
                    fields[3].parse().ok()?,
                    fields[4].parse().ok()?,
                ))
            })();
            if decoded.is_some() {
                return decoded;
            }
        }
        // Legacy names can start with "2|"; their delimiter suffix prevents
        // them from being mistaken for a valid version-two record.
    }
    let parent_idx = rest.find(":parent=")?;
    let name = &rest[..parent_idx];
    let rest = &rest[parent_idx + ":parent=".len()..];
    let fork_idx = rest.find(":fork=")?;
    let parent_text = &rest[..fork_idx];
    let rest = &rest[fork_idx + ":fork=".len()..];
    let head_idx = rest.find(":head=")?;
    let fork_text = &rest[..head_idx];
    let head_text = &rest[head_idx + ":head=".len()..];
    let fork: i64 = fork_text.parse().ok()?;
    let head: i64 = head_text.parse().ok()?;
    let parent = if parent_text.is_empty() {
        None
    } else {
        Some(parent_text.to_string())
    };
    Some(BranchInfo::new(name.to_string(), parent, fork, head))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vectors() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../docs/protocol/version-metadata-vectors.json"
        ))
        .unwrap()
    }

    #[test]
    fn shared_branch_vectors_preserve_names_and_legacy_records() {
        for value in vectors()["branches"].as_array().unwrap() {
            let expected = BranchInfo::new(
                value["name"].as_str().unwrap().into(),
                value["parent"].as_str().map(str::to_string),
                value["fork"].as_i64().unwrap(),
                value["head"].as_i64().unwrap(),
            );
            let marker = value["marker"].as_str().unwrap();
            assert_eq!(
                try_decode_branch_marker(marker),
                Some(expected.clone()),
                "{marker}"
            );
            if value["version"] == 2 {
                assert_eq!(encode_branch_marker(&expected), marker);
            }
        }
    }

    #[test]
    fn shared_malformed_branch_vectors_are_rejected() {
        for marker in vectors()["invalidBranches"].as_array().unwrap() {
            assert!(
                try_decode_branch_marker(marker.as_str().unwrap()).is_none(),
                "{marker}"
            );
        }
    }

    #[test]
    fn version_name_decoder_accepts_lowercase_and_rejects_invalid_utf16() {
        assert_eq!(decode_version_name("006a006b"), Some("jk".into()));
        for invalid in [
            "D800",
            "DFFF",
            "D8000041",
            "éé",
            "\u{ff10}\u{ff10}\u{ff10}\u{ff10}",
        ] {
            assert!(decode_version_name(invalid).is_none(), "{invalid}");
        }
    }

    #[test]
    fn encode_round_trips_through_decode() {
        let info = BranchInfo::new("feature".into(), Some("main".into()), 5, 9);
        let text = encode_branch_marker(&info);
        let decoded = try_decode_branch_marker(&text).unwrap();
        assert_eq!(info, decoded);
    }

    #[test]
    fn make_version_control_database_filename_returns_sibling_path() {
        let path =
            VersionControlDecorator::make_version_control_database_filename("/var/data/db.links");
        assert_eq!(path, PathBuf::from("/var/data/db.versioncontrol.links"));
    }

    #[test]
    fn decode_branch_marker_rejects_invalid_input() {
        assert!(try_decode_branch_marker("not a marker").is_none());
        assert!(try_decode_branch_marker("__vc:branch:x:parent=:fork=z:head=1").is_none());
    }
}
